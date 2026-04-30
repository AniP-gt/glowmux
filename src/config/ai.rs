use serde::{Deserialize, Serialize};

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
pub struct AiTitleEngineConfig {
    pub backend: String,
    /// Model override for claude-headless backend (e.g. "claude-haiku-4-5-20251001").
    /// Empty string uses the claude CLI default.
    pub model: String,
    pub max_chars: usize,
    pub timeout_sec: u64,
    pub update_interval_sec: u64,
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
