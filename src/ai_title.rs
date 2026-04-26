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

/// Returns true if a line looks like a shell prompt (end-of-command signal).
/// Used as fallback trigger for title generation in non-Claude panes.
pub fn is_shell_prompt(line: &str) -> bool {
    let t = line.trim_end();
    t.ends_with("$ ") || t.ends_with("% ") || t == "$" || t == "%"
        || t.ends_with("❯ ") || t.ends_with("❯")
}

/// Returns true if a line is UI noise that should not be stored in the ring buffer.
/// Filters out Claude Code status bars, bypass lines, ANSI artifacts, etc.
pub fn is_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return true;
    }
    // Claude Code status bar patterns
    if t.contains("bypass permissions") { return true; }
    if t.contains("shift+tab to cycle") { return true; }
    if t.contains("Update available! Run:") { return true; }
    if t.contains("brew upgrade claude-code") { return true; }
    // Lines that are pure punctuation/separators
    if t.chars().all(|c| matches!(c, '─' | '━' | '═' | '|' | '│' | '┤' | '├' | ' ' | '·')) {
        return true;
    }
    // Very short lines (single char noise)
    if t.chars().count() < 3 {
        return true;
    }
    false
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

    // Take last 3000 chars of meaningful output (most recent context)
    let sanitized: String = pane_output
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let sanitized = if sanitized.len() > 3000 {
        sanitized[sanitized.len() - 3000..].to_string()
    } else {
        sanitized
    };

    let max = config.max_chars;
    let prompt = format!(
        "以下はターミナルセッションの出力です。\
このセッションで何をしているか、{max}文字以内の日本語で端的に答えてください。\
タイトルのみ返し、句読点・引用符・説明文は不要です。\n\n{sanitized}"
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

    result.and_then(|s| {
        let trimmed = s.trim().to_string();
        // Strip surrounding quotes if the model returned them
        let trimmed = trimmed.trim_matches(|c| c == '"' || c == '「' || c == '」' || c == '\'').to_string();
        if trimmed.is_empty() {
            None
        } else if trimmed.chars().count() > max + 5 {
            Some(trimmed.chars().take(max).collect())
        } else {
            Some(trimmed)
        }
    })
}

