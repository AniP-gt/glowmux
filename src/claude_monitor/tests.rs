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
