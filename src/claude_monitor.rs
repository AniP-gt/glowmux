//! Claude Code session monitoring via JSONL transcript files.
//!
//! Watches ~/.claude/projects/<project>/*.jsonl for real-time events:
//! tool uses, sub-agent spawns (isSidechain), thinking state.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// A single todo item from TodoWrite tool.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub content: String,
    pub status: String, // "pending", "in_progress", "completed"
}

/// Current state of a Claude session inferred from JSONL events.
#[derive(Debug, Clone, Default)]
pub struct ClaudeState {
    pub current_tool: Option<String>,
    pub subagent_count: usize,
    pub subagent_types: Vec<String>,
    pub is_working: bool,
    pub tool_use_count: usize,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub todos: Vec<TodoItem>,
    pub context_tokens: u64,
    pub git_branch: Option<String>,
}

impl ClaudeState {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_creation_tokens
    }

    pub fn todo_progress(&self) -> (usize, usize) {
        let completed = self.todos.iter().filter(|t| t.status == "completed").count();
        (completed, self.todos.len())
    }

    /// Context window limit for the current model (in tokens).
    ///
    /// Claude Code writes the plain model id (e.g. `claude-opus-4-6`)
    /// into the JSONL without the `[1m]` suffix. Opus 4.6 ships with 1M
    /// context by default; Sonnet and Haiku default to 200K.
    pub fn context_limit(&self) -> u64 {
        match self.model.as_deref() {
            Some(m) if m.contains("[1m]") || m.contains("-1m") => 1_000_000,
            Some(m) if m.contains("opus-4-6") => 1_000_000,
            _ => 200_000,
        }
    }

    pub fn context_usage(&self) -> f64 {
        (self.context_tokens as f64 / self.context_limit() as f64).min(1.0)
    }

    pub fn short_model(&self) -> Option<&str> {
        let full = self.model.as_deref()?;
        if full.contains("opus") {
            Some("opus")
        } else if full.contains("sonnet") {
            Some("sonnet")
        } else if full.contains("haiku") {
            Some("haiku")
        } else {
            Some(full)
        }
    }
}

/// Per-pane monitor state.
struct PaneMonitor {
    bound_transcript_path: Option<PathBuf>,
    session_id: Option<String>,
    jsonl_path: Option<PathBuf>,
    file_position: u64,
    last_mtime: Option<SystemTime>,
    last_check: Instant,
    last_rescan: Instant,
    state: ClaudeState,
    /// Active sub-agents: tool_use_id → subagent_type (or "general-purpose")
    active_task_ids: std::collections::HashMap<String, String>,
    /// Request IDs already counted for token usage (avoid double-counting).
    counted_request_ids: std::collections::HashSet<String>,
}

impl PaneMonitor {
    fn new() -> Self {
        Self {
            bound_transcript_path: None,
            session_id: None,
            jsonl_path: None,
            file_position: 0,
            last_mtime: None,
            last_check: Instant::now() - Duration::from_secs(10),
            last_rescan: Instant::now() - Duration::from_secs(60),
            state: ClaudeState::default(),
            active_task_ids: std::collections::HashMap::new(),
            counted_request_ids: std::collections::HashSet::new(),
        }
    }
}

/// Shared state across all panes being monitored.
#[derive(Clone, Default)]
pub struct ClaudeMonitor {
    inner: Arc<Mutex<HashMap<usize, PaneMonitor>>>,
}

const CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// Maximum cached request IDs for token dedup. Clearing is safe — JSONL is
/// read sequentially and old lines are never re-read.
const MAX_REQUEST_ID_CACHE: usize = 10_000;

impl ClaudeMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self, pane_id: usize) -> ClaudeState {
        self.inner
            .lock()
            .ok()
            .and_then(|m| m.get(&pane_id).map(|p| p.state.clone()))
            .unwrap_or_default()
    }

    pub fn bind_session(
        &self,
        pane_id: usize,
        transcript_path: Option<&Path>,
        session_id: Option<&str>,
    ) {
        if let Ok(mut map) = self.inner.lock() {
            let monitor = map.entry(pane_id).or_insert_with(PaneMonitor::new);
            let next_session_id = session_id
                .filter(|id| !id.is_empty())
                .map(ToString::to_string);
            let session_changed = next_session_id.as_deref() != monitor.session_id.as_deref();
            if let Some(session_id) = next_session_id {
                monitor.session_id = Some(session_id);
            }

            let next_path = transcript_path
                .filter(|path| !path.as_os_str().is_empty())
                .and_then(normalize_transcript_path_candidate);

            if let Some(new_path) = next_path {
                let path_changed = monitor.bound_transcript_path.as_ref() != Some(&new_path);
                monitor.bound_transcript_path = Some(new_path.clone());
                if path_changed || session_changed {
                    reset_monitor_session(monitor, Some(new_path));
                }
            } else if session_changed {
                reset_monitor_session(monitor, monitor.bound_transcript_path.clone());
            }
        }
    }

    /// Update monitoring for a pane with its current cwd.
    /// Throttled to CHECK_INTERVAL to avoid per-frame syscalls.
    pub fn update(&self, pane_id: usize, cwd: &Path, allow_cwd_fallback: bool) {
        // Phase 1: check if we should run at all (short lock)
        let (path_to_read, read_from) = {
            let mut map = match self.inner.lock() {
                Ok(m) => m,
                Err(_) => return,
            };

            let monitor = map.entry(pane_id).or_insert_with(PaneMonitor::new);

            if monitor.last_check.elapsed() < CHECK_INTERVAL {
                return;
            }
            monitor.last_check = Instant::now();

            let path = match resolve_monitor_path(monitor, cwd, allow_cwd_fallback) {
                Some(path) => path,
                None => return,
            };

            let (path, meta) = match safe_transcript_file(&path) {
                Some(result) => result,
                None => return,
            };

            if monitor.jsonl_path.as_ref() != Some(&path) {
                reset_monitor_session(monitor, Some(path.clone()));
            }

            let mtime = meta.modified().ok();
            if mtime == monitor.last_mtime {
                return;
            }
            monitor.last_mtime = mtime;

            // File truncation/rotation detection: if file shrank, reset state
            if meta.len() < monitor.file_position {
                monitor.file_position = 0;
                monitor.state = ClaudeState::default();
                monitor.active_task_ids.clear();
                monitor.counted_request_ids.clear();
            }

            (path, monitor.file_position)
        };

        // Phase 2: read file without holding the lock
        let file = match File::open(&path_to_read) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(read_from)).is_err() {
            return;
        }

        let mut new_lines = Vec::new();
        let mut new_position = read_from;
        let mut buf = String::new();
        loop {
            buf.clear();
            let bytes = match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            if !buf.ends_with('\n') {
                break;
            }
            new_position += bytes as u64;
            new_lines.push(buf.clone());
        }

        // Phase 3: apply parsed events (short lock)
        if new_lines.is_empty() {
            return;
        }
        if let Ok(mut map) = self.inner.lock() {
            if let Some(monitor) = map.get_mut(&pane_id) {
                monitor.file_position = new_position;
                for line in &new_lines {
                    process_event(monitor, line);
                }
            }
        }
    }

    pub fn remove(&self, pane_id: usize) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&pane_id);
        }
    }

    #[cfg(test)]
    pub fn set_state_for_test(&self, pane_id: usize, state: ClaudeState) {
        if let Ok(mut map) = self.inner.lock() {
            let monitor = map.entry(pane_id).or_insert_with(PaneMonitor::new);
            monitor.state = state;
        }
    }
}

fn default_transcript_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("projects"))
}

fn normalize_transcript_path_candidate(path: &Path) -> Option<PathBuf> {
    normalize_transcript_path_candidate_with_root(path, &default_transcript_root()?)
}

fn normalize_transcript_path_candidate_with_root(
    path: &Path,
    transcript_root: &Path,
) -> Option<PathBuf> {
    if !path.is_absolute() || path.extension()? != "jsonl" {
        return None;
    }
    let canonical_root = transcript_root.canonicalize().ok()?;
    let canonical_parent = path.parent()?.canonicalize().ok()?;
    if !canonical_parent.starts_with(&canonical_root) {
        return None;
    }
    Some(canonical_parent.join(path.file_name()?))
}

fn safe_transcript_file(path: &Path) -> Option<(PathBuf, std::fs::Metadata)> {
    safe_transcript_file_with_root(path, &default_transcript_root()?)
}

fn safe_transcript_file_with_root(
    path: &Path,
    transcript_root: &Path,
) -> Option<(PathBuf, std::fs::Metadata)> {
    let normalized = normalize_transcript_path_candidate_with_root(path, transcript_root)?;
    let sym_meta = std::fs::symlink_metadata(&normalized).ok()?;
    if sym_meta.is_symlink() || !sym_meta.is_file() {
        return None;
    }
    Some((normalized.clone(), std::fs::metadata(&normalized).ok()?))
}

fn resolve_monitor_path(
    monitor: &mut PaneMonitor,
    cwd: &Path,
    allow_cwd_fallback: bool,
) -> Option<PathBuf> {
    if let Some(bound_path) = monitor.bound_transcript_path.clone() {
        if monitor.jsonl_path.as_ref() != Some(&bound_path) {
            reset_monitor_session(monitor, Some(bound_path.clone()));
        }
        return Some(bound_path);
    }

    if !allow_cwd_fallback {
        if monitor.jsonl_path.is_some() {
            reset_monitor_session(monitor, None);
        }
        return None;
    }

    let path_missing = monitor.jsonl_path.as_ref().is_none_or(|p| !p.exists());
    let stale_scan = monitor.last_rescan.elapsed() > Duration::from_secs(5);
    if path_missing || stale_scan {
        monitor.last_rescan = Instant::now();
        let expected_path = find_jsonl_path(cwd);
        if monitor.jsonl_path != expected_path {
            reset_monitor_session(monitor, expected_path);
        }
    }

    monitor.jsonl_path.clone()
}

fn reset_monitor_session(monitor: &mut PaneMonitor, jsonl_path: Option<PathBuf>) {
    monitor.jsonl_path = jsonl_path;
    monitor.file_position = 0;
    monitor.last_mtime = None;
    monitor.state = ClaudeState::default();
    monitor.active_task_ids.clear();
    monitor.counted_request_ids.clear();
}

fn process_event(monitor: &mut PaneMonitor, line: &str) {
    let json: serde_json::Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };

    let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        "assistant" => {
            let message = json.get("message");

            match message.and_then(|m| m.get("stop_reason")).and_then(|v| v.as_str()) {
                Some("tool_use") | None => monitor.state.is_working = true,
                Some(_) => {
                    monitor.state.is_working = false;
                    monitor.state.current_tool = None;
                }
            }

            if let Some(model) = message.and_then(|m| m.get("model")).and_then(|v| v.as_str()) {
                monitor.state.model = Some(model.to_string());
            }

            // Count tokens once per requestId to avoid double-counting duplicated JSONL lines.
            let should_count = json
                .get("requestId")
                .and_then(|v| v.as_str())
                .map(|id| {
                    if monitor.counted_request_ids.len() >= MAX_REQUEST_ID_CACHE {
                        monitor.counted_request_ids.clear();
                    }
                    monitor.counted_request_ids.insert(id.to_string())
                })
                .unwrap_or(false);

            if should_count {
                if let Some(usage) = message.and_then(|m| m.get("usage")) {
                    let g = |k| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
                    let (input, output, cr, cc) = (
                        g("input_tokens"),
                        g("output_tokens"),
                        g("cache_read_input_tokens"),
                        g("cache_creation_input_tokens"),
                    );
                    monitor.state.input_tokens += input;
                    monitor.state.output_tokens += output;
                    monitor.state.cache_read_tokens += cr;
                    monitor.state.cache_creation_tokens += cc;
                    // Current context = input + cache (total tokens sent this turn)
                    monitor.state.context_tokens = input + cr + cc;
                }
            }

            if let Some(branch) = json.get("gitBranch").and_then(|v| v.as_str()) {
                if !branch.is_empty() && branch != "HEAD" {
                    monitor.state.git_branch = Some(branch.to_string());
                }
            }

            if let Some(content) = message.and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                        continue;
                    }
                    if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                        monitor.state.current_tool = Some(name.to_string());
                        monitor.state.tool_use_count += 1;
                        monitor.state.is_working = true;

                        if name == "Agent" || name == "Task" {
                            if let Some(task_id) = block.get("id").and_then(|v| v.as_str()) {
                                let subagent_type = block
                                    .get("input")
                                    .and_then(|i| i.get("subagent_type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("general-purpose")
                                    .to_string();
                                monitor.active_task_ids.insert(task_id.to_string(), subagent_type);
                                monitor.state.subagent_count = monitor.active_task_ids.len();
                                monitor.state.subagent_types =
                                    monitor.active_task_ids.values().cloned().collect();
                            }
                        }

                        if name == "TodoWrite" {
                            if let Some(arr) = block
                                .get("input")
                                .and_then(|v| v.get("todos"))
                                .and_then(|v| v.as_array())
                            {
                                monitor.state.todos = arr
                                    .iter()
                                    .filter_map(|t| {
                                        Some(TodoItem {
                                            content: t.get("content")?.as_str()?.to_string(),
                                            status: t.get("status")?.as_str()?.to_string(),
                                        })
                                    })
                                    .collect();
                            }
                        }
                    }
                }
            }
        }
        "user" => {
            let content = json
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array());

            let mut has_tool_result = false;
            if let Some(content) = content {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        has_tool_result = true;
                        if let Some(id) = block.get("tool_use_id").and_then(|v| v.as_str()) {
                            if monitor.active_task_ids.remove(id).is_some() {
                                monitor.state.subagent_count = monitor.active_task_ids.len();
                                monitor.state.subagent_types =
                                    monitor.active_task_ids.values().cloned().collect();
                            }
                        }
                    }
                }
            }

            if !has_tool_result {
                monitor.state.is_working = false;
                monitor.state.current_tool = None;
            }
        }
        _ => {}
    }
}

/// Convert a cwd path to Claude's project directory name and find the most recent JSONL.
fn find_jsonl_path(cwd: &Path) -> Option<PathBuf> {
    let project_dir = dirs::home_dir()?
        .join(".claude")
        .join("projects")
        .join(encode_cwd_to_project_name(cwd));

    if !project_dir.exists() {
        return None;
    }

    std::fs::read_dir(&project_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ex| ex == "jsonl"))
        .filter_map(|e| e.metadata().ok()?.modified().ok().map(|m| (e.path(), m)))
        .max_by_key(|(_, m)| *m)
        .map(|(p, _)| p)
}

/// Encode a path to Claude's project name format.
/// Claude Code replaces any non-ASCII-alphanumeric character with `-`.
/// E.g., `/Users/tk/workspace/github.com/foo` → `-Users-tk-workspace-github-com-foo`
fn encode_cwd_to_project_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_cwd() {
        assert_eq!(encode_cwd_to_project_name(&PathBuf::from(r"C:\Users\foo\bar")), "C--Users-foo-bar");
    }

    #[test]
    fn test_encode_cwd_dot_in_path() {
        // Dots (e.g. in "github.com") are encoded as dashes, matching Claude Code's behaviour.
        assert_eq!(
            encode_cwd_to_project_name(&PathBuf::from("/Users/tk/workspace/github.com/foo/bar")),
            "-Users-tk-workspace-github-com-foo-bar"
        );
    }

    #[test]
    fn test_encode_cwd_japanese() {
        // Non-ASCII chars are encoded as dashes.
        assert_eq!(
            encode_cwd_to_project_name(&PathBuf::from("C:\\Users\\じゅぶ\\dev\\ccmux")),
            "C--Users-----dev-ccmux"
        );
    }

    #[test]
    fn test_process_tool_use() {
        let mut monitor = PaneMonitor::new();
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","id":"toolu_001","input":{}}],"stop_reason":"tool_use"}}"#;
        process_event(&mut monitor, line);
        assert_eq!(monitor.state.current_tool.as_deref(), Some("Bash"));
        assert!(monitor.state.is_working);
    }

    #[test]
    fn test_process_agent_spawn_and_complete() {
        let mut monitor = PaneMonitor::new();
        process_event(&mut monitor, r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","id":"toolu_agent1","input":{}}],"stop_reason":"tool_use"}}"#);
        assert_eq!(monitor.state.subagent_count, 1);
        process_event(&mut monitor, r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_agent1","content":"done"}]}}"#);
        assert_eq!(monitor.state.subagent_count, 0);
    }

    #[test]
    fn test_token_usage_no_double_count() {
        let mut monitor = PaneMonitor::new();
        let line = r#"{"type":"assistant","requestId":"req_123","message":{"model":"claude-opus-4-6","content":[{"type":"tool_use","name":"Bash","id":"t1","input":{}}],"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":1000}}}"#;
        process_event(&mut monitor, line);
        process_event(&mut monitor, line);
        process_event(&mut monitor, line);
        assert_eq!(monitor.state.input_tokens, 100);
        assert_eq!(monitor.state.output_tokens, 50);
        assert_eq!(monitor.state.cache_read_tokens, 1000);
    }

    #[test]
    fn test_reset_monitor_session_clears_cached_state() {
        let mut monitor = PaneMonitor::new();
        monitor.file_position = 123;
        monitor.last_mtime = Some(SystemTime::now());
        monitor.state.output_tokens = 42;
        monitor.active_task_ids.insert("task-1".to_string(), "explore".to_string());
        monitor.counted_request_ids.insert("req-1".to_string());

        reset_monitor_session(&mut monitor, Some(PathBuf::from("/tmp/session.jsonl")));

        assert_eq!(monitor.jsonl_path.as_deref(), Some(Path::new("/tmp/session.jsonl")));
        assert_eq!(monitor.file_position, 0);
        assert!(monitor.last_mtime.is_none());
        assert_eq!(monitor.state.total_tokens(), 0);
        assert!(monitor.active_task_ids.is_empty());
        assert!(monitor.counted_request_ids.is_empty());
    }

    #[test]
    fn test_bind_session_persists_explicit_transcript_path() {
        let monitor = ClaudeMonitor::new();
        let root = default_transcript_root()
            .unwrap()
            .join(format!("glowmux-claude-monitor-bind-{}", std::process::id()));
        let project_dir = root.join("repo");
        let transcript_path = project_dir.join("pane-a.jsonl");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(&transcript_path, b"").unwrap();

        monitor.bind_session(7, Some(&transcript_path), Some("session-7"));

        let normalized = normalize_transcript_path_candidate(&transcript_path).unwrap();
        let inner = monitor.inner.lock().unwrap();
        let pane = inner.get(&7).unwrap();
        assert_eq!(pane.bound_transcript_path.as_deref(), Some(normalized.as_path()));
        assert_eq!(pane.session_id.as_deref(), Some("session-7"));
        assert_eq!(pane.jsonl_path.as_deref(), Some(normalized.as_path()));

        drop(inner);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_normalize_transcript_path_candidate_rejects_outside_root() {
        let root = std::env::temp_dir()
            .join(format!("glowmux-claude-monitor-reject-{}", std::process::id()));
        let outside_dir = std::env::temp_dir()
            .join(format!("glowmux-claude-monitor-outside-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();

        assert!(normalize_transcript_path_candidate_with_root(
            &outside_dir.join("session.jsonl"),
            &root
        )
        .is_none());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside_dir);
    }

    #[test]
    fn test_todo_parsing() {
        let mut monitor = PaneMonitor::new();
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"TodoWrite","id":"t1","input":{"todos":[{"content":"Task A","status":"completed","activeForm":"Doing A"},{"content":"Task B","status":"in_progress","activeForm":"Doing B"},{"content":"Task C","status":"pending","activeForm":"Doing C"}]}}]}}"#;
        process_event(&mut monitor, line);
        assert_eq!(monitor.state.todos.len(), 3);
        assert_eq!(monitor.state.todo_progress(), (1, 3));
    }

    #[test]
    fn test_stop_reason_end_turn_clears_working() {
        let mut monitor = PaneMonitor::new();
        monitor.state.is_working = true;
        monitor.state.current_tool = Some("Bash".to_string());
        process_event(&mut monitor, r#"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}],"stop_reason":"end_turn"}}"#);
        assert!(!monitor.state.is_working);
        assert!(monitor.state.current_tool.is_none());
    }

    #[test]
    fn test_context_limit() {
        // Claude Code logs plain model id without [1m] suffix; opus-4-6 is 1M by default.
        let cases = [
            ("claude-opus-4-6", 1_000_000),
            ("claude-opus-4-6[1m]", 1_000_000),
            ("claude-opus-4-5", 200_000),
            ("claude-sonnet-4-6", 200_000),
            ("claude-haiku-4-5", 200_000),
        ];
        for (model, expected) in cases {
            let mut s = ClaudeState::default();
            s.model = Some(model.to_string());
            assert_eq!(s.context_limit(), expected, "model={model}");
        }
    }
}
