use std::time::Duration;
use tokio::time::timeout;

pub async fn invoke_claude_headless(prompt: &str, timeout_secs: u64) -> Option<String> {
    invoke_claude_headless_with_model(prompt, timeout_secs, "").await
}

pub async fn invoke_claude_headless_with_model(prompt: &str, timeout_secs: u64, model: &str) -> Option<String> {
    let model = model.to_string();
    let result = timeout(
        Duration::from_secs(timeout_secs),
        run_claude_headless(prompt, &model),
    )
    .await;

    result.unwrap_or_default()
}

async fn run_claude_headless(prompt: &str, model: &str) -> Option<String> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut cmd = tokio::process::Command::new("claude");
    cmd.arg("--print");
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(prompt.as_bytes()).await.is_err() {
            let _ = child.kill().await;
            return None;
        }
        drop(stdin);
    }

    let mut stdout_bytes = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let mut limited = (&mut stdout).take(65536);
        let _ = limited.read_to_end(&mut stdout_bytes).await;
    }

    let status = child.wait().await.ok()?;
    if !status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub async fn invoke_ollama(
    url: &str,
    model: &str,
    prompt: &str,
    timeout_secs: u64,
) -> Option<String> {
    let result = timeout(
        Duration::from_secs(timeout_secs),
        run_ollama(url, model, prompt),
    )
    .await;

    result.unwrap_or_default()
}

async fn run_ollama(url: &str, model: &str, prompt: &str) -> Option<String> {
    let endpoint = format!("{}/api/generate", url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false
    });
    let endpoint_clone = endpoint.clone();
    let body_str = body.to_string();
    let result = tokio::task::spawn_blocking(move || {
        ureq::post(&endpoint_clone)
            .set("Content-Type", "application/json")
            .send_string(&body_str)
            .ok()
            .and_then(|r| r.into_json::<serde_json::Value>().ok())
            .and_then(|j| j["response"].as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
    })
    .await
    .ok()
    .flatten();
    result
}

pub async fn invoke_gemini(
    api_key: &str,
    model: &str,
    prompt: &str,
    timeout_secs: u64,
) -> Option<String> {
    if api_key.is_empty() {
        return None;
    }
    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        run_gemini(api_key.to_string(), model.to_string(), prompt.to_string()),
    )
    .await;
    result.unwrap_or_default()
}

async fn run_gemini(api_key: String, model: String, prompt: String) -> Option<String> {
    // Validate model name to prevent URL injection (allowlist: alphanumeric, dash, dot)
    if !model.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.') {
        return None;
    }
    // API key is passed as a header, not in the URL, to avoid exposure in process listings/logs
    let endpoint = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }]
    });
    let body_str = body.to_string();
    let result = tokio::task::spawn_blocking(move || {
        ureq::post(&endpoint)
            .set("Content-Type", "application/json")
            .set("x-goog-api-key", &api_key)
            .send_string(&body_str)
            .ok()
            .and_then(|r| r.into_json::<serde_json::Value>().ok())
            .and_then(|j| {
                j["candidates"]
                    .get(0)
                    .and_then(|c| c["content"]["parts"].get(0))
                    .and_then(|p| p["text"].as_str())
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty())
    })
    .await
    .ok()
    .flatten();
    result
}
