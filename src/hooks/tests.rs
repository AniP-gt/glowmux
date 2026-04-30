use super::*;
use std::path::Path;

#[test]
fn test_hook_event_accepts_documented_event_names() {
    assert_eq!(
        HookEvent::from_str("PreToolUse"),
        Some(HookEvent::PreToolUse)
    );
    assert_eq!(
        HookEvent::from_str("UserPromptSubmit"),
        Some(HookEvent::UserPromptSubmit)
    );
    assert_eq!(
        HookEvent::from_str("Notification"),
        Some(HookEvent::Notification)
    );
}

#[test]
fn test_hook_message_parses_session_metadata() {
    let msg: HookMessage = serde_json::from_str(
        r#"{
            "hook_event_name": "Stop",
            "pane_id": 7,
            "transcript_path": "/tmp/claude-session.jsonl",
            "session_id": "session-123"
        }"#,
    )
    .unwrap();

    assert_eq!(msg.hook_event_name.as_deref(), Some("Stop"));
    assert_eq!(msg.pane_id, Some(7));
    assert_eq!(
        msg.transcript_path.as_deref(),
        Some(Path::new("/tmp/claude-session.jsonl"))
    );
    assert_eq!(msg.session_id.as_deref(), Some("session-123"));
}

#[test]
fn test_hook_event_falls_back_to_hook_event_name_when_event_is_unrecognized() {
    let msg: HookMessage = serde_json::from_str(
        r#"{
            "event": "post_tool",
            "hook_event_name": "Notification",
            "pane_id": 2
        }"#,
    )
    .unwrap();

    let hook_event = msg
        .event
        .as_deref()
        .and_then(HookEvent::from_str)
        .or_else(|| msg.hook_event_name.as_deref().and_then(HookEvent::from_str));

    assert_eq!(hook_event, Some(HookEvent::Notification));
}
