use super::*;

#[test]
fn test_default_config() {
    let config = ConfigFile::default();
    assert!(config.features.ai_title);
    assert!(config.features.status_dot);
    assert!(config.features.status_bg_color);
    assert!(config.features.status_bar);
    assert!(config.features.zoom);
    assert!(!config.preview.prefer_delta);
    assert_eq!(config.terminal.scrollback, 10000);
    assert_eq!(config.layout.breakpoint_stack, 120);
    assert_eq!(config.ai.provider, "ollama");
    assert_eq!(config.keybindings.zoom, "alt+z");
}

#[test]
fn test_partial_toml_parse() {
    let toml_str = r#"
[terminal]
scrollback = 5000

[features]
ai_title = false

[preview]
prefer_delta = true
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.terminal.scrollback, 5000);
    assert!(!config.features.ai_title);
    assert!(config.preview.prefer_delta);
    assert!(!config.features.auto_worktree);
    assert_eq!(config.layout.file_tree_width, 20);
}

#[test]
fn test_empty_toml_parse() {
    let config: ConfigFile = toml::from_str("").unwrap();
    assert_eq!(config.terminal.scrollback, 10000);
    assert_eq!(config.ai.provider, "ollama");
}

#[test]
fn test_load_returns_default_when_no_file() {
    // This test verifies that load() returns valid config (either default or from file).
    // We can't assert specific default values since the user's config file may exist.
    let config = ConfigFile::load();
    assert!(config.terminal.scrollback > 0);
}

#[test]
fn test_startup_panes_parse() {
    let toml_str = r#"
[startup]
enabled = true
default_agent = "claude"

[[startup.panes]]
command = "claude"
worktree = false
branch = ""
split = "horizontal"

[[startup.panes]]
command = "claude"
worktree = false
branch = ""
split = "horizontal"
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.startup.enabled);
    assert_eq!(config.startup.panes.len(), 2);
    assert_eq!(config.startup.panes[0].command, "claude");
    assert_eq!(config.startup.panes[0].split, "horizontal");
}

#[test]
fn test_startup_panes_roundtrip() {
    let toml_str = r#"
[startup]
enabled = true
default_agent = "claude"

[[startup.panes]]
command = "claude"
worktree = false
branch = ""
split = "horizontal"

[[startup.panes]]
command = "claude"
worktree = false
branch = ""
split = "horizontal"
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.startup.panes.len(), 2);

    let serialized = toml::to_string_pretty(&config).unwrap();
    let config2: ConfigFile = toml::from_str(&serialized).unwrap();
    assert_eq!(config2.startup.enabled, true);
    assert_eq!(config2.startup.panes.len(), 2, "panes lost after round-trip");
}
