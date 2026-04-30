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
