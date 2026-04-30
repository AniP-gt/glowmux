use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    Arg,
    Flag(String),
    Stdin,
    None,
}

impl Default for PromptMode {
    fn default() -> Self {
        PromptMode::None
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MultiAiAgent {
    pub name: String,
    pub command: String,
    pub prompt_mode: PromptMode,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MultiAiConfig {
    pub agents: Vec<MultiAiAgent>,
}

impl Default for MultiAiConfig {
    fn default() -> Self {
        Self {
            agents: vec![
                MultiAiAgent {
                    name: "claude".into(),
                    command: "claude".into(),
                    prompt_mode: PromptMode::Arg,
                },
                MultiAiAgent {
                    name: "opencode".into(),
                    command: "opencode run".into(),
                    prompt_mode: PromptMode::Arg,
                },
                MultiAiAgent {
                    name: "gemini".into(),
                    command: "gemini".into(),
                    prompt_mode: PromptMode::Flag("-i".into()),
                },
                MultiAiAgent {
                    name: "codex".into(),
                    command: "codex".into(),
                    prompt_mode: PromptMode::Arg,
                },
            ],
        }
    }
}

impl MultiAiConfig {
    /// Drop empty-command agents and downgrade unsafe Flag values to None
    /// so that user-controlled flags cannot inject extra shell arguments.
    pub fn validated(mut self) -> Self {
        self.agents.retain(|a| !a.command.is_empty());
        self.agents.retain(|a| is_safe_command(&a.command));
        for agent in &mut self.agents {
            if let PromptMode::Flag(ref flag) = agent.prompt_mode {
                if !is_safe_flag(flag) {
                    agent.prompt_mode = PromptMode::None;
                }
            }
        }
        self
    }
}

/// Reject shell metacharacters in agent.command. Allows a single space-separated
/// subcommand token (e.g. "opencode run", "codex exec") but each token must start
/// with an alphanumeric character — this blocks flags (`--flag`), path traversal
/// (`../`), and shell metacharacters.
pub(super) fn is_safe_command(cmd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }
    cmd.split(' ').all(|token| {
        !token.is_empty()
            && token.starts_with(|c: char| c.is_alphanumeric())
            && token
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
            && !token.contains("..")
    })
}

pub(super) fn is_safe_flag(flag: &str) -> bool {
    let bytes = flag.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    if bytes[0] != b'-' {
        return false;
    }
    let rest = &bytes[1..];
    let after_dashes = if rest[0] == b'-' {
        if rest.len() < 2 {
            return false;
        }
        &rest[1..]
    } else {
        rest
    };
    if after_dashes.is_empty() {
        return false;
    }
    if !after_dashes[0].is_ascii_alphabetic() {
        return false;
    }
    after_dashes[1..]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_ai_default_agents() {
        let cfg = MultiAiConfig::default();
        assert_eq!(cfg.agents.len(), 4);
        assert_eq!(cfg.agents[0].name, "claude");
        assert_eq!(cfg.agents[0].command, "claude");
        assert_eq!(cfg.agents[0].prompt_mode, PromptMode::Arg);
        assert_eq!(cfg.agents[1].name, "opencode");
        assert_eq!(cfg.agents[1].command, "opencode run");
        assert_eq!(cfg.agents[1].prompt_mode, PromptMode::Arg);
        assert_eq!(cfg.agents[2].name, "gemini");
        assert_eq!(cfg.agents[2].prompt_mode, PromptMode::Flag("-i".into()));
        assert_eq!(cfg.agents[3].name, "codex");
        assert_eq!(cfg.agents[3].command, "codex");
        assert_eq!(cfg.agents[3].prompt_mode, PromptMode::Arg);
    }

    #[test]
    fn test_multi_ai_toml_roundtrip() {
        let orig = MultiAiConfig::default();
        let s = toml::to_string(&orig).unwrap();
        let parsed: MultiAiConfig = toml::from_str(&s).unwrap();
        assert_eq!(parsed.agents.len(), 4);
        assert_eq!(parsed.agents[2].prompt_mode, PromptMode::Flag("-i".into()));
    }

    #[test]
    fn test_flag_validation_rejects_injection() {
        let cfg = MultiAiConfig {
            agents: vec![MultiAiAgent {
                name: "evil".into(),
                command: "evil".into(),
                prompt_mode: PromptMode::Flag("--flag 'x'; evil".into()),
            }],
        };
        let validated = cfg.validated();
        assert_eq!(validated.agents[0].prompt_mode, PromptMode::None);
    }

    #[test]
    fn test_command_validation_rejects_injection() {
        let cfg = MultiAiConfig {
            agents: vec![
                MultiAiAgent {
                    name: "evil".into(),
                    command: "claude ; rm -rf ~".into(),
                    prompt_mode: PromptMode::Arg,
                },
                MultiAiAgent {
                    name: "ok".into(),
                    command: "claude".into(),
                    prompt_mode: PromptMode::Arg,
                },
            ],
        };
        let validated = cfg.validated();
        assert_eq!(validated.agents.len(), 1);
        assert_eq!(validated.agents[0].name, "ok");
    }

    #[test]
    fn test_command_validation_accepts_subcommands() {
        for cmd in &["opencode run", "codex exec", "my-tool sub"] {
            assert!(is_safe_command(cmd), "should accept subcommand: {}", cmd);
        }
        for cmd in &["opencode run --flag", "codex exec; evil", "cmd && evil"] {
            assert!(!is_safe_command(cmd), "should reject: {}", cmd);
        }
    }

    #[test]
    fn test_flag_validation_accepts_common_flags() {
        for flag in &["-p", "--prompt", "-x", "--flag-name", "--flag_name"] {
            assert!(is_safe_flag(flag), "should accept {}", flag);
        }
        for flag in &[
            "", "-", "--", "p", "-1", "--1bad", "-p x", "-p;rm", "-p\nrm",
        ] {
            assert!(!is_safe_flag(flag), "should reject {:?}", flag);
        }
    }
}
