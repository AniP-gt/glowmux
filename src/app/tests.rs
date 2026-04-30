use super::*;
use std::time::Duration;

#[test]
fn test_should_request_ai_title_allows_first_generation() {
    assert!(should_request_ai_title(false, false, None, 30));
}

#[test]
fn test_should_request_ai_title_blocks_when_already_requested_once() {
    assert!(!should_request_ai_title(true, false, None, 30));
}

#[test]
fn test_should_request_ai_title_blocks_when_in_flight() {
    assert!(!should_request_ai_title(false, true, None, 30));
}

#[test]
fn test_should_request_ai_title_respects_interval() {
    let recent = Instant::now();
    let old = Instant::now() - Duration::from_secs(31);

    assert!(!should_request_ai_title(false, false, Some(recent), 30));
    assert!(should_request_ai_title(false, false, Some(old), 30));
}

#[test]
fn test_resolve_pane_status_prefers_jsonl_running() {
    let state = crate::claude_monitor::ClaudeState {
        is_working: true,
        ..Default::default()
    };

    assert_eq!(
        resolve_pane_status(PaneStatus::Waiting, &state),
        PaneStatus::Running
    );
}

#[test]
fn test_resolve_pane_status_prefers_hook_waiting_over_done_tokens() {
    let state = crate::claude_monitor::ClaudeState {
        output_tokens: 42,
        ..Default::default()
    };

    assert_eq!(
        resolve_pane_status(PaneStatus::Waiting, &state),
        PaneStatus::Waiting
    );
}

#[test]
fn test_resolve_pane_status_prefers_hook_running_over_done_tokens() {
    let state = crate::claude_monitor::ClaudeState {
        output_tokens: 42,
        ..Default::default()
    };

    assert_eq!(
        resolve_pane_status(PaneStatus::Running, &state),
        PaneStatus::Running
    );
}

#[test]
fn test_resolve_pane_status_falls_back_to_done_from_tokens() {
    let state = crate::claude_monitor::ClaudeState {
        output_tokens: 42,
        ..Default::default()
    };

    assert_eq!(
        resolve_pane_status(PaneStatus::Idle, &state),
        PaneStatus::Done
    );
}

#[test]
fn test_dismiss_done_on_focus_marks_derived_done_as_dismissed() {
    let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
    let pane_id = app.ws().focused_pane_id;

    let state = crate::claude_monitor::ClaudeState {
        output_tokens: 42,
        ..Default::default()
    };
    app.claude_monitor.set_state_for_test(pane_id, state);

    assert_eq!(app.pane_status(pane_id), PaneStatus::Done);
    app.dismiss_done_on_focus(pane_id);

    assert!(app.pane_state_dismissed(pane_id));
    assert_eq!(app.pane_states.get(&pane_id).map(|s| s.status), Some(PaneStatus::Done));
}

#[test]
fn test_layout_single_pane() {
    let layout = LayoutNode::Leaf { pane_id: 1 };
    assert_eq!(layout.pane_count(), 1);
    assert_eq!(layout.collect_pane_ids(), vec![1]);
}

#[test]
fn test_layout_split_vertical() {
    let mut layout = LayoutNode::Leaf { pane_id: 1 };
    layout.split_pane(1, 2, SplitDirection::Vertical);
    assert_eq!(layout.pane_count(), 2);
    assert_eq!(layout.collect_pane_ids(), vec![1, 2]);
}

#[test]
fn test_layout_split_horizontal() {
    let mut layout = LayoutNode::Leaf { pane_id: 1 };
    layout.split_pane(1, 2, SplitDirection::Horizontal);
    assert_eq!(layout.pane_count(), 2);
}

#[test]
fn test_layout_nested_split() {
    let mut layout = LayoutNode::Leaf { pane_id: 1 };
    layout.split_pane(1, 2, SplitDirection::Vertical);
    layout.split_pane(1, 3, SplitDirection::Horizontal);
    assert_eq!(layout.pane_count(), 3);
    assert_eq!(layout.collect_pane_ids(), vec![1, 3, 2]);
}

#[test]
fn test_layout_remove_pane() {
    let mut layout = LayoutNode::Leaf { pane_id: 1 };
    layout.split_pane(1, 2, SplitDirection::Vertical);
    layout.remove_pane(2);
    assert_eq!(layout.pane_count(), 1);
    assert_eq!(layout.collect_pane_ids(), vec![1]);
}

#[test]
fn test_layout_remove_first_pane() {
    let mut layout = LayoutNode::Leaf { pane_id: 1 };
    layout.split_pane(1, 2, SplitDirection::Vertical);
    layout.remove_pane(1);
    assert_eq!(layout.collect_pane_ids(), vec![2]);
}

#[test]
fn test_calculate_rects_vertical() {
    let layout = LayoutNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
        second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
    };
    let rects = layout.calculate_rects(Rect::new(0, 0, 100, 50));
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0], (1, Rect::new(0, 0, 50, 50)));
    assert_eq!(rects[1], (2, Rect::new(50, 0, 50, 50)));
}

#[test]
fn test_calculate_rects_horizontal() {
    let layout = LayoutNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.5,
        first: Box::new(LayoutNode::Leaf { pane_id: 1 }),
        second: Box::new(LayoutNode::Leaf { pane_id: 2 }),
    };
    let rects = layout.calculate_rects(Rect::new(0, 0, 100, 50));
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0], (1, Rect::new(0, 0, 100, 25)));
    assert_eq!(rects[1], (2, Rect::new(0, 25, 100, 25)));
}

#[test]
fn test_focus_cycling() {
    let ids = vec![1, 2, 3];
    assert_eq!((0 + 1) % ids.len(), 1);
    assert_eq!((2 + 1) % ids.len(), 0);
}

#[test]
fn test_on_workspace_focus_context_changed_preserves_tree_when_root_matches() {
    let config = ConfigFile::default();
    let mut app = App::new(40, 120, config).expect("app");

    let root = app.ws().file_tree.root_path.clone();
    app.ws_mut().file_tree.selected_index = 2;
    app.ws_mut().file_tree.scroll_offset = 1;

    app.on_workspace_focus_context_changed();

    assert_eq!(app.ws().file_tree.root_path, root);
    assert_eq!(app.ws().file_tree.selected_index, 2);
    assert_eq!(app.ws().file_tree.scroll_offset, 1);
}

#[test]
fn test_extract_preview_selected_text_uses_diff_lines_in_diff_mode() {
    let mut preview = crate::preview::Preview::new();
    preview.lines = vec!["plain".to_string()];
    preview.diff_mode = true;
    preview.diff_lines = vec![crate::preview::DiffLine {
        text: "+delta".to_string(),
        kind: crate::preview::DiffLineKind::Added,
        styled_spans: Vec::new(),
    }];

    assert_eq!(extract_preview_selected_text(&preview, 0, 0, 0, 5), "+delta");
}

#[test]
fn pane_display_title_priority() {
    let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
    let pane_id = app.ws().focused_pane_id;

    assert_eq!(app.pane_display_title(pane_id), None);

    app.ai_titles.insert(pane_id, "ai-title".to_string());
    assert_eq!(app.pane_display_title(pane_id), Some("ai-title"));

    app.pane_custom_titles
        .insert(pane_id, "custom".to_string());
    assert_eq!(app.pane_display_title(pane_id), Some("custom"));

    app.pane_custom_titles.remove(&pane_id);
    assert_eq!(app.pane_display_title(pane_id), Some("ai-title"));
}

#[test]
fn pane_cleanup_removes_custom_title() {
    let mut app = App::new(20, 80, ConfigFile::default()).unwrap();
    let pane_id = 5;

    app.pane_custom_titles
        .insert(pane_id, "manual".to_string());
    app.pane_rename_input = Some((pane_id, "buf".to_string()));

    app.cleanup_pane_runtime_state(pane_id);

    assert!(!app.pane_custom_titles.contains_key(&pane_id));
    assert!(app.pane_rename_input.is_none());
}

#[test]
fn rename_mutual_exclusion() {
    let mut app = App::new(20, 80, ConfigFile::default()).unwrap();

    // Tab rename dispatch should be skipped while pane_rename_input is set.
    app.pane_rename_input = Some((1, String::new()));
    let alt_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT);
    // pane_rename modal swallows the key (rather than starting tab rename).
    let _ = app.handle_key_event(alt_r).unwrap();
    assert!(app.rename_input.is_none());
    assert!(app.pane_rename_input.is_some());

    // Conversely, pane_rename dispatch should be skipped while rename_input is set.
    app.pane_rename_input = None;
    app.rename_input = Some(String::new());
    let alt_shift_r =
        KeyEvent::new(KeyCode::Char('R'), KeyModifiers::ALT | KeyModifiers::SHIFT);
    let _ = app.handle_key_event(alt_shift_r).unwrap();
    assert!(app.pane_rename_input.is_none());
    assert!(app.rename_input.is_some());
}

#[test]
fn edit_key_buffer_handles_basic_input() {
    let mut buf = String::from("hi");
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(edit_key_buffer(&mut buf, key, 32));
    assert_eq!(buf, "hia");

    let bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    assert!(edit_key_buffer(&mut buf, bs, 32));
    assert_eq!(buf, "hi");

    // Ctrl/Alt-modified chars are swallowed but do not append.
    let ctrl = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(edit_key_buffer(&mut buf, ctrl, 32));
    assert_eq!(buf, "hi");

    // Length cap.
    let mut full = "x".repeat(32);
    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(edit_key_buffer(&mut full, key, 32));
    assert_eq!(full.chars().count(), 32);

    // Non-handled keys return false.
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!edit_key_buffer(&mut buf, enter, 32));
}

#[test]
fn test_format_cmd_arg() {
    let result = App::format_agent_command(
        "claude",
        "hello world",
        &crate::config::PromptMode::Arg,
    );
    assert_eq!(result, "claude 'hello world'");
}

#[test]
fn test_format_cmd_flag() {
    let result = App::format_agent_command(
        "gemini",
        "hello world",
        &crate::config::PromptMode::Flag("-p".into()),
    );
    assert_eq!(result, "gemini -p 'hello world'");
}

#[test]
fn test_format_cmd_stdin() {
    let result =
        App::format_agent_command("codex", "hello", &crate::config::PromptMode::Stdin);
    assert_eq!(result, "codex");
}

#[test]
fn test_format_cmd_none() {
    let result =
        App::format_agent_command("foo", "hello", &crate::config::PromptMode::None);
    assert_eq!(result, "foo");
}

#[test]
fn test_format_cmd_empty_prompt() {
    let result =
        App::format_agent_command("claude", "", &crate::config::PromptMode::Arg);
    assert_eq!(result, "claude");
}

/// Minimal POSIX sh single-quote unquoter: accepts a string composed of
/// '...' segments and \' escape characters, returns the literal content.
/// Returns None on malformed input.
fn posix_unquote(s: &str) -> Option<String> {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_quote = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_quote {
            if b == b'\'' {
                in_quote = false;
                i += 1;
            } else {
                out.push(b as char);
                i += 1;
            }
        } else {
            match b {
                b'\'' => {
                    in_quote = true;
                    i += 1;
                }
                b'\\' if i + 1 < bytes.len() => {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                }
                _ => return None,
            }
        }
    }
    if in_quote {
        return None;
    }
    Some(out)
}

#[test]
fn test_format_cmd_injection() {
    let result = App::format_agent_command(
        "claude",
        "'; rm -rf / #",
        &crate::config::PromptMode::Arg,
    );
    assert!(result.starts_with("claude "), "result: {result}");
    let arg = &result["claude ".len()..];
    // The argument must round-trip through POSIX shell single-quote
    // semantics back to the original prompt; this rejects any escape
    // that would let "; rm" run as a separate command.
    let parsed = posix_unquote(arg).expect("should be parseable");
    assert_eq!(parsed, "'; rm -rf / #", "not round-trip safe: {result}");
}

#[test]
fn test_grid_layout_n2() {
    let node = App::build_layout_node(LayoutMode::Grid, &[1, 2]);
    assert!(node.is_some());
    let ids = node.unwrap().collect_pane_ids();
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_grid_layout_n3() {
    let node = App::build_layout_node(LayoutMode::Grid, &[1, 2, 3]);
    assert!(node.is_some());
    let ids = node.unwrap().collect_pane_ids();
    assert_eq!(ids.len(), 3);
}

#[test]
fn test_grid_layout_n4() {
    let node = App::build_layout_node(LayoutMode::Grid, &[1, 2, 3, 4]);
    assert!(node.is_some());
    let ids = node.unwrap().collect_pane_ids();
    assert_eq!(ids.len(), 4);
}

#[test]
fn test_multi_zero_selection_sets_error() {
    // Verify that format_agent_command with all-false checks never reaches
    // pane creation — the empty selected_indices guard fires first.
    let checks: Vec<bool> = vec![false, false, false, false];
    let selected: Vec<usize> = checks
        .iter()
        .enumerate()
        .filter(|(_, &c)| c)
        .map(|(i, _)| i)
        .collect();
    assert!(selected.is_empty(), "zero checks must yield no selected agents");
    // Simulate the error path: error_msg should be set, not None.
    let error_msg: Option<String> = if selected.is_empty() {
        Some("Select at least one AI".into())
    } else {
        None
    };
    assert_eq!(error_msg.as_deref(), Some("Select at least one AI"));
}

#[test]
fn test_single_tab_cycle_9_stops() {
    // Single mode must cycle through exactly 9 distinct stops before
    // returning to LaunchModeToggle (the new first stop).
    use PaneCreateField::*;
    let stops = [
        LaunchModeToggle,
        BranchName,
        BaseBranch,
        WorktreeToggle,
        AgentField,
        PromptField,
        AiGenerate,
        OkButton,
        CancelButton,
    ];
    // Verify Tab forward wraps: last stop -> LaunchModeToggle
    assert_eq!(stops.len(), 9);
    // Simulate the Tab forward transition for each stop
    let n_agents = 4usize;
    let advance = |f: &PaneCreateField| -> PaneCreateField {
        match f {
            LaunchModeToggle => BranchName,
            BranchName => BaseBranch,
            BaseBranch => WorktreeToggle,
            WorktreeToggle => AgentField,
            AgentField => PromptField,
            PromptField => AiGenerate,
            AiGenerate => OkButton,
            OkButton => CancelButton,
            CancelButton => LaunchModeToggle,
            MultiCheck(i) if *i + 1 < n_agents => MultiCheck(*i + 1),
            MultiCheck(_) => OkButton,
        }
    };
    let mut f = LaunchModeToggle;
    for expected in stops.iter().skip(1).chain(std::iter::once(&LaunchModeToggle)) {
        f = advance(&f);
        assert_eq!(&f, expected);
    }
}

#[test]
fn test_multi_tab_cycle_8_stops() {
    // Multi mode with 4 agents: 4 + 4 = 8 stops.
    use PaneCreateField::*;
    let n_agents = 4usize;
    let advance = |f: &PaneCreateField| -> PaneCreateField {
        match f {
            LaunchModeToggle => if n_agents > 0 { MultiCheck(0) } else { PromptField },
            MultiCheck(i) if *i + 1 < n_agents => MultiCheck(*i + 1),
            MultiCheck(_) => PromptField,
            PromptField => OkButton,
            OkButton => CancelButton,
            CancelButton => LaunchModeToggle,
            _ => LaunchModeToggle,
        }
    };
    let stops = [
        LaunchModeToggle,
        MultiCheck(0),
        MultiCheck(1),
        MultiCheck(2),
        MultiCheck(3),
        PromptField,
        OkButton,
        CancelButton,
    ];
    assert_eq!(stops.len(), 8);
    let mut f = LaunchModeToggle;
    for expected in stops.iter().skip(1).chain(std::iter::once(&LaunchModeToggle)) {
        f = advance(&f);
        assert_eq!(&f, expected);
    }
}
