use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub features: FeaturesConfig,
    pub terminal: TerminalConfig,
    pub layout: LayoutConfig,
    pub startup: StartupConfig,
    pub pane: PaneConfig,
    pub ai: AiConfig,
    pub status: StatusConfig,
    pub worktree: WorktreeConfig,
    pub session: SessionConfig,
    pub keybindings: KeybindingsConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FeaturesConfig {
    pub ai_title: bool,
    pub ai_worktree_name: bool,
    pub auto_worktree: bool,
    pub session_restore: bool,
    pub feature_toggle_ui: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub scrollback: usize,
    pub unicode_width: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub auto_responsive: bool,
    pub breakpoint_stack: u16,
    pub breakpoint_split2: u16,
    pub file_tree_width: u16,
    pub preview_width: u16,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StartupConfig {
    pub enabled: bool,
    pub panes: Vec<StartupPane>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StartupPane {
    pub command: String,
    pub worktree: bool,
    pub branch: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PaneConfig {
    pub border_style: String,
    pub show_pane_numbers: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub provider: String,
    pub title: AiTitleConfig,
    pub worktree_name: AiWorktreeNameConfig,
    pub ollama: AiOllamaConfig,
    pub gemini: AiGeminiConfig,
    pub claude_headless: AiClaudeHeadlessConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiTitleConfig {
    pub enabled: bool,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiWorktreeNameConfig {
    pub enabled: bool,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiOllamaConfig {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiGeminiConfig {
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiClaudeHeadlessConfig {
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StatusConfig {
    pub color_running: String,
    pub color_done: String,
    pub color_waiting: String,
    pub bg_done: String,
    pub bg_waiting: String,
    pub bg_reset: String,
    pub indicator: String,
    pub respect_terminal_bg: bool,
    pub override_bg_done: String,
    pub override_bg_waiting: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WorktreeConfig {
    pub base_dir: String,
    pub auto_branch: bool,
    pub branch_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub enabled: bool,
    pub auto_save: bool,
    pub save_interval: u64,
    pub restore_on_start: bool,
}

/// Key binding configuration. Fields are parsed from config.toml but runtime
/// wiring is planned for Iteration 2. Currently the default values document
/// the intended bindings; changing them in config.toml has no effect yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub zoom: String,
    pub layout_cycle: String,
    pub layout_picker: String,
    pub pane_left: String,
    pub pane_right: String,
    pub pane_up: String,
    pub pane_down: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback: 10000,
            unicode_width: true,
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            auto_responsive: true,
            breakpoint_stack: 120,
            breakpoint_split2: 200,
            file_tree_width: 20,
            preview_width: 40,
        }
    }
}

impl Default for PaneConfig {
    fn default() -> Self {
        Self {
            border_style: "rounded".to_string(),
            show_pane_numbers: true,
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            title: AiTitleConfig::default(),
            worktree_name: AiWorktreeNameConfig::default(),
            ollama: AiOllamaConfig::default(),
            gemini: AiGeminiConfig::default(),
            claude_headless: AiClaudeHeadlessConfig::default(),
        }
    }
}

impl Default for AiTitleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: String::new(),
            prompt: "Generate a short title for this terminal session".to_string(),
        }
    }
}

impl Default for AiWorktreeNameConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: String::new(),
            prompt: "Generate a short branch name for this worktree".to_string(),
        }
    }
}

impl Default for AiOllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
        }
    }
}

impl Default for AiGeminiConfig {
    fn default() -> Self {
        Self {
            model: "gemini-2.0-flash".to_string(),
        }
    }
}

impl Default for AiClaudeHeadlessConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-5".to_string(),
        }
    }
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            color_running: "cyan".to_string(),
            color_done: "green".to_string(),
            color_waiting: "yellow".to_string(),
            bg_done: "#1a2e1a".to_string(),
            bg_waiting: "#2e2a1a".to_string(),
            bg_reset: "reset".to_string(),
            indicator: "\u{25cf}".to_string(),
            respect_terminal_bg: true,
            override_bg_done: String::new(),
            override_bg_waiting: String::new(),
        }
    }
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            base_dir: "~/worktrees".to_string(),
            auto_branch: true,
            branch_prefix: "feat/".to_string(),
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_save: true,
            save_interval: 30,
            restore_on_start: true,
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            zoom: "alt+z".to_string(),
            layout_cycle: "ctrl+space".to_string(),
            layout_picker: "ctrl+l".to_string(),
            pane_left: "alt+h".to_string(),
            pane_right: "alt+l".to_string(),
            pane_up: "alt+k".to_string(),
            pane_down: "alt+j".to_string(),
        }
    }
}

impl ConfigFile {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if let Some(path) = config_path {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        match toml::from_str::<ConfigFile>(&content) {
                            Ok(config) => return config,
                            Err(e) => {
                                eprintln!("glowmux: config parse error (using defaults): {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("glowmux: config read error (using defaults): {}", e);
                    }
                }
            }
        }
        ConfigFile::default()
    }

    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("glowmux").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ConfigFile::default();
        assert!(!config.features.ai_title);
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
ai_title = true
"#;
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(config.terminal.scrollback, 5000);
        assert!(config.features.ai_title);
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
        let config = ConfigFile::load();
        assert_eq!(config.terminal.scrollback, 10000);
    }
}
