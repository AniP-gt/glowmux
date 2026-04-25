use std::path::PathBuf;
use std::sync::mpsc::Sender;

use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

use crate::app::AppEvent;

#[derive(Debug, Clone, PartialEq)]
pub enum HookEvent {
    Stop,
    UserPromptSubmit,
    PreToolUse,
    Notification,
}

impl HookEvent {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "stop" => Some(Self::Stop),
            "user_prompt_submit" => Some(Self::UserPromptSubmit),
            "pre_tool_use" => Some(Self::PreToolUse),
            "notification" => Some(Self::Notification),
            _ => None,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct HookMessage {
    event: String,
    pane_id: Option<usize>,
}

pub struct HookServerGuard {
    socket_path: PathBuf,
}

impl Drop for HookServerGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

pub fn socket_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("glowmux").join("hooks.sock"))
}

pub async fn start_hook_server(tx: Sender<AppEvent>, socket_path: PathBuf) {
    if let Some(parent) = socket_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let listener = match bind_with_retry(&socket_path).await {
        Some(l) => l,
        None => {
            eprintln!(
                "glowmux: hook server could not bind to {} (continuing without hooks)",
                socket_path.display()
            );
            return;
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            &socket_path,
            std::fs::Permissions::from_mode(0o600),
        );
    }

    let _guard = HookServerGuard {
        socket_path: socket_path.clone(),
    };

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            handle_connection(stream, tx).await;
        });
    }
}

async fn bind_with_retry(path: &std::path::Path) -> Option<UnixListener> {
    match UnixListener::bind(path) {
        Ok(l) => Some(l),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let path_owned = path.to_path_buf();
            let is_live = tokio::task::spawn_blocking(move || {
                std::os::unix::net::UnixStream::connect(&path_owned).is_ok()
            })
            .await
            .unwrap_or(false);
            if is_live {
                eprintln!("glowmux: another process is using {}", path.display());
                return None;
            }
            let _ = std::fs::remove_file(path);
            UnixListener::bind(path).ok()
        }
        Err(e) => {
            eprintln!("glowmux: failed to bind hook socket: {}", e);
            None
        }
    }
}

async fn handle_connection(mut stream: tokio::net::UnixStream, tx: Sender<AppEvent>) {
    let mut buf = Vec::new();
    let mut limited = (&mut stream).take(65536);

    if limited.read_to_end(&mut buf).await.is_err() {
        return;
    }

    let text = match std::str::from_utf8(&buf) {
        Ok(t) => t.trim(),
        Err(_) => return,
    };

    let msg: HookMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(_) => return,
    };

    let pane_id = match msg.pane_id {
        Some(id) => id,
        None => return,
    };

    if let Some(hook_event) = HookEvent::from_str(&msg.event) {
        let _ = tx.send(AppEvent::HookReceived {
            pane_id,
            event: hook_event,
        });
    }
}
