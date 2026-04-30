use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

mod pty_io;

use crate::app::AppEvent;
use self::pty_io::pty_reader_thread;
use crate::shell::detect_shell;

enum WriterMsg {
    Data(Vec<u8>),
    Shutdown,
}

/// A terminal pane wrapping a PTY and vt100 parser.
pub struct Pane {
    pub id: usize,
    master: Box<dyn MasterPty + Send>,
    /// Channel to the background writer thread — avoids blocking the main event loop.
    writer_tx: SyncSender<WriterMsg>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    child: Box<dyn Child + Send + Sync>,
    _reader_handle: thread::JoinHandle<()>,
    _writer_handle: thread::JoinHandle<()>,
    last_rows: u16,
    last_cols: u16,
    pub exited: bool,
    pub title: Arc<Mutex<String>>,
    pub cwd: PathBuf,
    pub total_scrollback: Arc<std::sync::atomic::AtomicUsize>,
    pub worktree_path: Option<PathBuf>,
    pub branch_name: Option<String>,
    /// Agent command to launch after worktree cd completes (e.g. "claude")
    pub pending_agent: Option<String>,
}

impl Pane {
    /// Create a new pane with a PTY shell.
    pub fn new(id: usize, rows: u16, cols: u16, event_tx: Sender<AppEvent>) -> Result<Self> {
        Self::new_with_cwd(id, rows, cols, event_tx, None)
    }

    pub fn new_with_cwd(
        id: usize,
        rows: u16,
        cols: u16,
        event_tx: Sender<AppEvent>,
        cwd: Option<PathBuf>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();

        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(pty_size).context("Failed to open PTY")?;

        let shell = detect_shell();
        let mut cmd = CommandBuilder::new(&shell);

        let shell_name = shell
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if shell_name.contains("bash") || shell_name.contains("zsh") {
            cmd.arg("--login");
        }

        let work_dir =
            cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        cmd.cwd(&work_dir);
        cmd.env("TERM", "xterm-256color");
        cmd.env("GLOWMUX", "1"); // marker to detect nested glowmux
        cmd.env("GLOWMUX_PANE_ID", id.to_string()); // pane ID for hook scripts

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;

        // Drop the slave side — we only use master
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .context("Failed to take PTY writer")?;

        // Scrollback buffer: 10000 lines of history
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10000)));
        let pane_title = Arc::new(Mutex::new(String::new()));

        let reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;

        let parser_clone = Arc::clone(&parser);
        let title_clone = Arc::clone(&pane_title);
        let scrollback_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let scrollback_clone = Arc::clone(&scrollback_counter);
        let reader_handle = thread::Builder::new()
            .name(format!("pty-reader-{}", id))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pty_reader_thread(
                        reader,
                        parser_clone,
                        title_clone,
                        scrollback_clone,
                        id,
                        event_tx,
                    );
                }));
                if let Err(e) = result {
                    let msg = if let Some(s) = e.downcast_ref::<&str>() {
                        format!("pane {} reader thread panicked: {}", id, s)
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        format!("pane {} reader thread panicked: {}", id, s)
                    } else {
                        format!("pane {} reader thread panicked (unknown payload)", id)
                    };
                    crate::core::log::write_log("PANIC", &msg);
                }
            })
            .context("Failed to spawn reader thread")?;

        // Background writer thread: drains the channel and writes to PTY.
        // This prevents blocking the main event loop when the PTY buffer is full.
        // Capacity of 64 messages gives burst headroom while still providing
        // backpressure. WriterMsg::Shutdown causes the thread to exit cleanly.
        let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(64);
        let writer_handle = thread::Builder::new()
            .name(format!("pty-writer-{}", id))
            .spawn(move || {
                let mut w = writer;
                for msg in writer_rx {
                    match msg {
                        WriterMsg::Shutdown => break,
                        WriterMsg::Data(chunk) => {
                            if w.write_all(&chunk).is_err() || w.flush().is_err() {
                                crate::core::log::write_log(
                                    "WARN",
                                    &format!("pane {} writer: PTY write failed, stopping", id),
                                );
                                break;
                            }
                        }
                    }
                }
            })
            .context("Failed to spawn writer thread")?;

        let mut pane = Self {
            id,
            master: pair.master,
            writer_tx,
            parser,
            child,
            _reader_handle: reader_handle,
            _writer_handle: writer_handle,
            last_rows: rows,
            last_cols: cols,
            exited: false,
            title: pane_title,
            cwd: work_dir,
            total_scrollback: scrollback_counter,
            worktree_path: None,
            branch_name: None,
            pending_agent: None,
        };

        // Inject OSC 7 hook after shell starts
        // Leading space prevents it from appearing in bash history
        if shell_name.contains("bash") {
            let setup = concat!(
                " __glowmux_osc7() { printf '\\033]7;file://%s%s\\007' \"$HOSTNAME\" \"$PWD\"; };",
                " PROMPT_COMMAND=\"__glowmux_osc7;${PROMPT_COMMAND}\";",
                " clear\n",
            );
            let _ = pane.write_input(setup.as_bytes());
        } else if shell_name.contains("zsh") {
            let setup = concat!(
                " __glowmux_osc7() { printf '\\033]7;file://%s%s\\007' \"$HOST\" \"$PWD\"; };",
                " precmd_functions+=(__glowmux_osc7);",
                " clear\n",
            );
            let _ = pane.write_input(setup.as_bytes());
        }

        Ok(pane)
    }

    /// Write input bytes to the PTY (keyboard input from user).
    ///
    /// Sends to the background writer thread via a bounded channel so the main
    /// event loop is never blocked by a full PTY buffer.  If the channel is
    /// full (the child process has stopped reading input), we drop the chunk
    /// and mark the pane exited so the UI can show it is dead.
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        if self.exited {
            return Ok(());
        }
        match self.writer_tx.try_send(WriterMsg::Data(data.to_vec())) {
            Ok(_) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                // Channel is full: the child process is not consuming input fast enough.
                // We drop this chunk to avoid blocking the main event loop.  A single
                // dropped keystroke is visible to the user (character does not appear)
                // which is a better outcome than the UI freezing entirely.
                crate::core::log::write_log(
                    "WARN",
                    &format!("pane {} writer channel full — input dropped", self.id),
                );
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                crate::core::log::write_log(
                    "WARN",
                    &format!("pane {} writer thread disconnected", self.id),
                );
                self.exited = true;
            }
        }
        Ok(())
    }

    /// Resize the PTY and vt100 parser. Returns `true` if the size
    /// actually changed (useful for callers that want to know whether
    /// a SIGWINCH was sent to the child). No-op and returns `false`
    /// when the size hasn't changed.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<bool> {
        if rows == 0 || cols == 0 {
            return Ok(false);
        }

        // Skip if size hasn't changed
        if rows == self.last_rows && cols == self.last_cols {
            return Ok(false);
        }

        self.last_rows = rows;
        self.last_cols = cols;

        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")?;

        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        parser.screen_mut().set_size(rows, cols);
        // Clear the screen buffer to avoid rendering stale content at the new size.
        // The TUI app (e.g. Claude Code) receives SIGWINCH and will redraw.
        // A brief blank frame is preferable to overlapping garbled output.
        parser.process(b"\x1b[2J\x1b[H");

        Ok(true)
    }

    /// Scroll the terminal view up (into scrollback history).
    pub fn scroll_up(&self, lines: usize) {
        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        let current = parser.screen().scrollback();
        parser.screen_mut().set_scrollback(current + lines);
    }

    /// Get scrollbar info: (current_offset, max_offset).
    /// max_offset is estimated by trying to scroll to a large value and checking.
    pub fn scrollbar_info(&self) -> (usize, usize) {
        let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        let screen = parser.screen();
        let current = screen.scrollback();
        // Estimate max by checking: set_scrollback clamps to actual scrollback length
        // We can't query it directly, so use the stored total_scrollback as estimate
        let total = self
            .total_scrollback
            .load(std::sync::atomic::Ordering::Relaxed);
        (current, total)
    }

    /// Scroll the terminal view down (towards current output).
    pub fn scroll_down(&self, lines: usize) {
        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        let current = parser.screen().scrollback();
        parser
            .screen_mut()
            .set_scrollback(current.saturating_sub(lines));
    }

    /// Reset scroll to the bottom (live view).
    pub fn scroll_reset(&self) {
        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        parser.screen_mut().set_scrollback(0);
    }

    /// Check if the terminal is scrolled back.
    pub fn is_scrolled_back(&self) -> bool {
        let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        parser.screen().scrollback() > 0
    }

    /// Check if the PTY application has enabled bracketed paste mode.
    pub fn is_bracketed_paste_enabled(&self) -> bool {
        let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        parser.screen().bracketed_paste()
    }

    /// Check if Claude Code is running in this pane (by window title).
    pub fn is_claude_running(&self) -> bool {
        if let Ok(t) = self.title.lock() {
            let lower = t.to_lowercase();
            lower.contains("claude")
        } else {
            false
        }
    }

    pub fn pane_cwd(&self) -> PathBuf {
        if let Some(ref wt) = self.worktree_path {
            return wt.clone();
        }
        self.cwd.clone()
    }

    /// Kill the PTY child process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        // Signal the writer thread to exit before killing the child, so it
        // flushes any buffered writes and releases the PTY fd cleanly.
        let _ = self.writer_tx.try_send(WriterMsg::Shutdown);
        self.kill();
    }
}
