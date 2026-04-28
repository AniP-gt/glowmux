mod ai_invoke;
mod ai_title;
mod app;
mod claude_monitor;
mod config;
mod filetree;
mod git_diff;
mod git_exec;
mod git_status;
mod hooks;
mod keybinding;
pub mod log;
mod pane;
mod preview;
mod runtime;
mod session;
mod ui;
mod version_check;
mod worktree;

use std::io;
use std::panic;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> Result<()> {
    // Detect if running inside another glowmux instance
    if std::env::var("GLOWMUX").is_ok() {
        eprintln!("glowmux: already running inside a glowmux pane (nested instance not allowed).");
        eprintln!(
            "         Open a new tab with Alt+T (or Ctrl+T) or split with Ctrl+D / Ctrl+E instead."
        );
        std::process::exit(1);
    }

    // If a directory is passed as argument, cd into it first
    if let Some(dir) = std::env::args().nth(1) {
        let path = std::path::Path::new(&dir);
        if path.is_dir() {
            std::env::set_current_dir(path)?;
        } else {
            eprintln!("glowmux: not a directory: {}", dir);
            std::process::exit(1);
        }
    }

    // Initialize file logger
    log::init();

    // Install panic hook to restore terminal state on crash
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), crossterm::event::DisableMouseCapture);
        let _ = execute!(io::stdout(), crossterm::event::DisableBracketedPaste);
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let msg = format!("PANIC: {}", info);
        log::write_log("PANIC", &msg);
        default_hook(info);
    }));

    // Initialize async runtime BEFORE raw mode
    let async_runtime = runtime::AsyncRuntime::new()?;

    // Query terminal for graphics protocol support BEFORE raw mode.
    // Falls back to halfblocks if detection fails.
    let image_picker = Some(
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks()),
    );

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    execute!(stdout, crossterm::event::EnableBracketedPaste)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Get initial terminal size
    let size = terminal.size()?;

    // Load configuration
    let config = config::ConfigFile::load();

    // Create app
    let mut app = app::App::new(size.height, size.width, config)?;
    app.image_picker = image_picker;
    app.tokio_handle = async_runtime.handle();
    app.refresh_git_status_for_render(true);

    // Start hook server on async runtime
    if let Some(socket_path) = hooks::socket_path() {
        let tx = app.event_tx.clone();
        async_runtime.spawn(hooks::start_hook_server(tx, socket_path));
    }

    // Keep runtime alive for the lifetime of the app
    let _async_runtime = async_runtime;

    // Main event loop
    let result = run_event_loop(&mut terminal, &mut app);

    // Save session before cleanup
    app.save_session();

    // Cleanup
    app.shutdown();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture
    )?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableBracketedPaste
    )?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    let mut paste_buffer: Vec<u8> = Vec::new();

    loop {
        // Drain any PTY output events
        app.drain_pty_events();

        // Auto-refresh file tree if sidebar is visible
        if app.ws().file_tree_visible && app.ws_mut().file_tree.auto_refresh_if_needed() {
            app.refresh_git_status_for_render(true);
            app.dirty = true;
        }

        // After paste, wait a few frames for PTY echo to settle
        if app.paste_cooldown > 0 {
            app.paste_cooldown -= 1;
            if app.paste_cooldown == 0 {
                app.dirty = true;
            }
        }

        // After a layout change (split/close/sidebar/terminal resize),
        // wait a few frames so child PTYs can respond to SIGWINCH with
        // a fresh redraw. Prevents the "old buffer at new size" flash.
        if app.resize_cooldown > 0 {
            app.resize_cooldown -= 1;
            if app.resize_cooldown == 0 {
                app.dirty = true;
            }
        }

        // Clear status flash after 2 seconds
        if let Some((_, when)) = &app.status_flash {
            if when.elapsed().as_secs() >= 2 {
                app.status_flash = None;
                app.dirty = true;
            }
        }

        // Only render when something changed (and no cooldown is active)
        if app.dirty && app.paste_cooldown == 0 && app.resize_cooldown == 0 {
            app.dirty = false;
            terminal.draw(|frame| {
                ui::render(app, frame);
            })?;
        }

        if app.should_quit {
            log::write_log("INFO", "glowmux exiting normally");
            break;
        }

        // Poll for crossterm events with a short timeout (~30fps)
        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        let consumed = app.handle_key_event(key)?;
                        if !consumed {
                            // Collect rapid key events as potential paste
                            if let Some(bytes) = crate::app::key_event_to_bytes_pub(&key) {
                                paste_buffer.extend_from_slice(&bytes);
                                // Drain all immediately available key events (paste burst)
                                while event::poll(Duration::from_millis(1))? {
                                    if let Event::Key(k) = event::read()? {
                                        if k.kind == KeyEventKind::Press {
                                            if app.handle_key_event(k)? {
                                                // Shortcut consumed — flush buffer first
                                                if !paste_buffer.is_empty() {
                                                    flush_paste_buffer(app, &mut paste_buffer)?;
                                                }
                                                break;
                                            }
                                            if let Some(b) = crate::app::key_event_to_bytes_pub(&k)
                                            {
                                                paste_buffer.extend_from_slice(&b);
                                            }
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                flush_paste_buffer(app, &mut paste_buffer)?;
                            }
                        }
                        app.dirty = true;
                    }
                }
                Event::Paste(text) => {
                    app.forward_paste_to_pty(&text)?;
                    app.paste_cooldown = 5;
                    app.dirty = true;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse_event(mouse);
                    app.dirty = true;
                }
                Event::Resize(cols, rows) => {
                    // Propagate the new terminal size to App so every
                    // pane's PTY gets a prompt SIGWINCH, and hold the
                    // paint for a few frames while the children redraw.
                    app.on_terminal_resize(cols, rows);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Flush accumulated key buffer to PTY. If multiple characters were collected
/// (indicating a paste), wrap in bracketed paste sequences only when the PTY
/// application has enabled the mode. Unconditional wrapping causes shells that
/// haven't opted in to display the escape sequences as literal text (issue #2).
fn flush_paste_buffer(app: &mut app::App, buffer: &mut Vec<u8>) -> Result<()> {
    if buffer.is_empty() {
        return Ok(());
    }

    let focused_id = app.ws().focused_pane_id;
    if let Some(pane) = app.ws_mut().panes.get_mut(&focused_id) {
        pane.scroll_reset();
        if buffer.len() > 6 {
            if pane.is_bracketed_paste_enabled() {
                let mut data = Vec::with_capacity(buffer.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend_from_slice(buffer);
                data.extend_from_slice(b"\x1b[201~");
                pane.write_input(&data)?;
            } else {
                pane.write_input(buffer)?;
            }
            app.paste_cooldown = 5;
        } else {
            // Normal typing — send directly
            pane.write_input(buffer)?;
        }
    }
    buffer.clear();
    Ok(())
}
