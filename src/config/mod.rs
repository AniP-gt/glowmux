use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod ai;
mod features;
mod multi_ai;

pub use ai::{AiConfig, AiTitleEngineConfig};
pub use features::FeaturesConfig;
pub use multi_ai::{MultiAiAgent, MultiAiConfig, PromptMode};

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
    pub preview: PreviewConfig,
    pub multi_ai: MultiAiConfig,
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
    pub default_agent: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StartupPane {
    pub command: String,
    pub worktree: bool,
    pub branch: String,
    pub split: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PaneConfig {
    pub border_style: String,
    pub show_pane_numbers: bool,
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
    pub main_branch: String,
    pub prefer_gwq: bool,
    pub gwq_basedir: String,
    pub worktree_dir: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SessionConfig {
    pub enabled: bool,
    pub auto_save: bool,
    pub save_interval: u64,
    pub restore_on_start: bool,
    pub restore_claude: bool,
    pub save_path: String,
}

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
    pub pane_rename: String,
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
    pub pane_list: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FileTreeConfig {
    pub enter_action: String,
    pub editor: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PreviewConfig {
    pub prefer_delta: bool,
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
            pane_rename: "alt+shift+r".to_string(),
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
            pane_next: "alt+n".to_string(),
            pane_prev: "alt+b".to_string(),
            pane_list: "alt+p".to_string(),
        }
    }
}

impl Default for FileTreeConfig {
    fn default() -> Self {
        Self {
            enter_action: "preview".to_string(),
            editor: "nvim".to_string(),
        }
    }
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            prefer_delta: false,
        }
    }
}

impl ConfigFile {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if let Some(path) = config_path {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str::<ConfigFile>(&content) {
                        Ok(mut config) => {
                            config.multi_ai = config.multi_ai.validated();
                            return config;
                        }
                        Err(e) => {
                            eprintln!("glowmux: config parse error (using defaults): {}", e);
                        }
                    },
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
mod tests;
