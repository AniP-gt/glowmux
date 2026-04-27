use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
    pub ai_title_engine: AiTitleEngineConfig,
    pub filetree: FileTreeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FeaturesConfig {
    pub ai_title: bool,
    pub ai_worktree_name: bool,
    pub auto_worktree: bool,
    pub session_restore: bool,
    pub feature_toggle_ui: bool,
    pub status_dot: bool,
    pub status_bg_color: bool,
    pub status_bar: bool,
    pub zoom: bool,
    // Spec v5 additions
    pub worktree: bool,
    pub worktree_ai_name: bool,
    pub file_tree: bool,
    pub file_preview: bool,
    pub diff_preview: bool,
    pub cd_tracking: bool,
    pub responsive_layout: bool,
    pub session_persist: bool,
    pub context_copy: bool,
    pub layout_picker: bool,
    pub startup_panes: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            ai_title: true,
            ai_worktree_name: false,
            auto_worktree: false,
            session_restore: false,
            feature_toggle_ui: false,
            status_dot: true,
            status_bg_color: true,
            status_bar: true,
            zoom: true,
            worktree: true,
            worktree_ai_name: false,
            file_tree: true,
            file_preview: true,
            diff_preview: false,
            cd_tracking: true,
            responsive_layout: true,
            session_persist: false,
            context_copy: true,
            layout_picker: true,
            startup_panes: true,
        }
    }
}

impl FeaturesConfig {
    pub fn get_by_key(&self, key: &str) -> bool {
        match key {
            "status_dot" => self.status_dot,
            "status_bg_color" => self.status_bg_color,
            "status_bar" => self.status_bar,
            "ai_title" => self.ai_title,
            "zoom" => self.zoom,
            "worktree" => self.worktree,
            "worktree_ai_name" => self.worktree_ai_name,
            "file_tree" => self.file_tree,
            "file_preview" => self.file_preview,
            "diff_preview" => self.diff_preview,
            "cd_tracking" => self.cd_tracking,
            "responsive_layout" => self.responsive_layout,
            "session_persist" => self.session_persist,
            "context_copy" => self.context_copy,
            "layout_picker" => self.layout_picker,
            "startup_panes" => self.startup_panes,
            _ => false,
        }
    }

    pub fn set_by_key(&mut self, key: &str, value: bool) {
        match key {
            "status_dot" => self.status_dot = value,
            "status_bg_color" => self.status_bg_color = value,
            "status_bar" => self.status_bar = value,
            "ai_title" => self.ai_title = value,
            "zoom" => self.zoom = value,
            "worktree" => self.worktree = value,
            "worktree_ai_name" => self.worktree_ai_name = value,
            "file_tree" => self.file_tree = value,
            "file_preview" => self.file_preview = value,
            "diff_preview" => self.diff_preview = value,
            "cd_tracking" => self.cd_tracking = value,
            "responsive_layout" => self.responsive_layout = value,
            "session_persist" => self.session_persist = value,
            "context_copy" => self.context_copy = value,
            "layout_picker" => self.layout_picker = value,
            "startup_panes" => self.startup_panes = value,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub scrollback: usize,
    pub unicode_width: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub auto_responsive: bool,
    pub breakpoint_stack: u16,
    pub breakpoint_split2: u16,
    pub file_tree_width: u16,
    pub preview_width: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StartupConfig {
    pub enabled: bool,
    pub panes: Vec<StartupPane>,
    /// Default agent command pre-filled in the Ctrl+N pane create dialog (e.g. "claude")
    pub default_agent: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StartupPane {
    pub command: String,
    pub worktree: bool,
    pub branch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PaneConfig {
    pub border_style: String,
    pub show_pane_numbers: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiConfig {
    pub provider: String,
    pub title: AiTitleConfig,
    pub worktree_name: AiWorktreeNameConfig,
    pub ollama: AiOllamaConfig,
    pub gemini: AiGeminiConfig,
    pub claude_headless: AiClaudeHeadlessConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiTitleConfig {
    pub enabled: bool,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiWorktreeNameConfig {
    pub enabled: bool,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiOllamaConfig {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiGeminiConfig {
    pub model: String,
    /// API key is never serialized back to disk to avoid plaintext credential leakage.
    /// Set via config.toml [ai.gemini] api_key; it will be read on load but never written.
    #[serde(default, skip_serializing)]
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiClaudeHeadlessConfig {
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WorktreeConfig {
    pub base_dir: String,
    pub auto_branch: bool,
    pub branch_prefix: String,
    pub close_confirm: bool,
    pub close_worktree: String,
    pub auto_create: bool,
    /// Default branch name for merge detection (e.g. "main", "master")
    pub main_branch: String,
    // Spec v5 additions
    /// Prefer the `gwq` CLI when available; falls back to `git worktree`.
    pub prefer_gwq: bool,
    /// Base directory for gwq-managed worktrees (e.g. "~/ghq").
    pub gwq_basedir: String,
    /// Subdirectory under the repo root for git-worktree-managed worktrees.
    pub worktree_dir: String,
    /// Default base branch for new worktrees (used by the pane create dialog).
    pub base_branch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SessionConfig {
    pub enabled: bool,
    pub auto_save: bool,
    pub save_interval: u64,
    pub restore_on_start: bool,
    // Spec v5 additions
    /// When true, restoring a session also re-launches the `claude` CLI in
    /// each restored pane that was running it.
    pub restore_claude: bool,
    /// Override path for the session save file. Empty falls back to the
    /// default `dirs::config_dir()/glowmux/session.json` location.
    pub save_path: String,
}

/// Key binding configuration. All fields are wired to runtime behavior.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub prefix: String,
    pub zoom: String,
    pub layout_cycle: String,
    pub layout_picker: String,
    pub pane_left: String,
    pub pane_right: String,
    pub pane_up: String,
    pub pane_down: String,
    pub quit: String,
    pub tab_rename: String,
    pub tab_new: String,
    pub tab_next: String,
    pub tab_prev: String,
    pub settings: String,
    pub file_tree: String,
    pub preview_swap: String,
    pub split_vertical: String,
    pub split_horizontal: String,
    pub pane_close: String,
    pub pane_create: String,
    pub clipboard_copy: String,
    pub ai_title_toggle: String,
    pub feature_toggle: String,
    pub pane_next: String,
    pub pane_prev: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FileTreeConfig {
    pub enter_action: String,
    // Editor command name only (no arguments, no shell metacharacters)
    pub editor: String,
}

impl Default for FileTreeConfig {
    fn default() -> Self {
        Self {
            enter_action: "preview".to_string(),
            editor: "nvim".to_string(),
        }
    }
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
            enabled: true,
            model: String::new(),
            prompt: "Generate a short title for this terminal session".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiTitleEngineConfig {
    pub backend: String,
    /// Model override for claude-headless backend (e.g. "claude-haiku-4-5-20251001").
    /// Empty string uses the claude CLI default.
    pub model: String,
    pub max_chars: usize,
    pub timeout_sec: u64,
    pub update_interval_sec: u64,
}

impl Default for AiTitleEngineConfig {
    fn default() -> Self {
        Self {
            backend: "claude-headless".to_string(),
            model: "claude-haiku-4-5-20251001".to_string(),
            max_chars: 20,
            timeout_sec: 5,
            update_interval_sec: 30,
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
            api_key: String::new(),
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
            close_confirm: false,
            close_worktree: "ask".to_string(),
            auto_create: false,
            main_branch: "main".to_string(),
            prefer_gwq: false,
            gwq_basedir: "~/ghq".to_string(),
            worktree_dir: ".glowmux".to_string(),
            base_branch: "main".to_string(),
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
            restore_claude: false,
            save_path: "~/.config/glowmux/session.json".to_string(),
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            prefix: "ctrl+b".to_string(),
            zoom: "alt+z".to_string(),
            layout_cycle: "ctrl+space".to_string(),
            layout_picker: "ctrl+l".to_string(),
            pane_left: "alt+h".to_string(),
            pane_right: "alt+l".to_string(),
            pane_up: "alt+k".to_string(),
            pane_down: "alt+j".to_string(),
            quit: "ctrl+q".to_string(),
            tab_rename: "alt+r".to_string(),
            tab_new: "ctrl+t".to_string(),
            tab_next: "alt+right".to_string(),
            tab_prev: "alt+left".to_string(),
            settings: "ctrl+,".to_string(),
            file_tree: "ctrl+f".to_string(),
            preview_swap: "ctrl+p".to_string(),
            split_vertical: "ctrl+d".to_string(),
            split_horizontal: "ctrl+e".to_string(),
            pane_close: "ctrl+w".to_string(),
            pane_create: "ctrl+n".to_string(),
            clipboard_copy: "ctrl+y".to_string(),
            ai_title_toggle: "alt+a".to_string(),
            feature_toggle: "?".to_string(),
            pane_next: "alt+]".to_string(),
            pane_prev: "alt+[".to_string(),
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

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Err("Cannot determine config path".into()),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &content)?;
        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e.into());
        }
        Ok(())
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
        assert!(config.features.ai_title);
        assert!(config.features.status_dot);
        assert!(config.features.status_bg_color);
        assert!(config.features.status_bar);
        assert!(config.features.zoom);
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
"#;
        let config: ConfigFile = toml::from_str(toml_str).unwrap();
        assert_eq!(config.terminal.scrollback, 5000);
        assert!(!config.features.ai_title);
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
