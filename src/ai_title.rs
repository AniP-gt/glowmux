use crate::ai_invoke;
use crate::config::AiTitleEngineConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum AiTitleBackend {
    ClaudeHeadless,
    Ollama,
}

impl AiTitleBackend {
    pub fn from_str(s: &str) -> Self {
        match s {
            "ollama" => Self::Ollama,
            _ => Self::ClaudeHeadless,
        }
    }
}

pub async fn generate_title(
    pane_output: &str,
    config: &AiTitleEngineConfig,
    ollama_url: &str,
    ollama_model: &str,
) -> Option<String> {
    if pane_output.trim().is_empty() {
        return None;
    }

    let sanitized: String = pane_output
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(2000)
        .collect();

    let prompt = format!(
        "以下のターミナル出力を{}文字以内の日本語で要約してください。タイトルのみ返してください。\n{}",
        config.max_chars, sanitized
    );

    let backend = AiTitleBackend::from_str(&config.backend);

    let result = match backend {
        AiTitleBackend::ClaudeHeadless => {
            ai_invoke::invoke_claude_headless_with_model(&prompt, config.timeout_sec, &config.model).await
        }
        AiTitleBackend::Ollama => {
            ai_invoke::invoke_ollama(ollama_url, ollama_model, &prompt, config.timeout_sec).await
        }
    };

    result.map(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.chars().count() > config.max_chars + 5 {
            trimmed.chars().take(config.max_chars).collect()
        } else {
            trimmed
        }
    })
}

pub fn detect_prompt_return(last_line: &str) -> bool {
    let trimmed = last_line.trim_end();
    // Standard shell prompts
    trimmed.ends_with("$ ")
        || trimmed.ends_with("> ")
        || trimmed.ends_with("% ")
        || trimmed == "$"
        || trimmed == ">"
        || trimmed == "%"
        // Claude Code interactive prompts
        || trimmed.ends_with("? ")
        || trimmed.ends_with("❯ ")
        || trimmed.ends_with("❯")
}
