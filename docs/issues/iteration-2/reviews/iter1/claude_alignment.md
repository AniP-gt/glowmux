## Review Result: REQUEST_CHANGES

### Blocking Issues (MUST FIX)

1. **[CRITICAL] reqwest added in violation of constraints** — Cargo.toml line 28 adds `reqwest = { version = "0.12", ... }` and `src/ai_invoke.rs` uses it for Ollama calls. The constraint explicitly states "ureq must NOT be replaced with reqwest". reqwest introduces an async HTTP client that duplicates the existing ureq dependency and violates the stated constraint.
   Fix: Remove reqwest from Cargo.toml. Rewrite `run_ollama` in `src/ai_invoke.rs` to either use a blocking ureq call on a spawn_blocking thread, or use tokio's own HTTP primitives. The simplest compliant approach is `tokio::task::spawn_blocking` with a ureq call.

2. **[MAJOR] `PaneStatus::Waiting` is never set** — The `Waiting` variant is defined in `PaneStatus` and rendered in the UI (yellow dot, background color), but the `HookReceived` handler in `app.rs:2292-2308` only sets `Running` and `Done`. The `"notification"` hook event (which is the only plausible trigger for `Waiting`) falls through the `_ => {}` arm without updating state. The spec requires `PaneStatus::Waiting` for the "Waiting" state triggered by a hook notification.
   Fix: Add `"notification" => { state.status = PaneStatus::Waiting; state.dismissed = false; }` to the `HookReceived` match arm.

3. **[MAJOR] AI title generation is never triggered** — `ai_title::detect_prompt_return` and `ai_title::generate_title` exist but are never called from `app.rs`. `ai_title_enabled` is toggled by Ctrl+A and displayed in the status bar, but no code path ever calls `generate_title` and sends `AppEvent::AiTitleGenerated`. `pane_output_rings` is populated but nothing reads it to invoke the AI pipeline. The feature is decorative only.
   Fix: In `drain_pty_events`, when processing PTY output for a pane and `ai_title_enabled` is true, check `detect_prompt_return` on the last ring line; if true, spawn a task via `async_runtime` (or use a `std::thread::spawn` with a blocking call) to call `generate_title` and send `AppEvent::AiTitleGenerated`. Since `AsyncRuntime` is owned by `main.rs`, a simpler approach is: store a `Sender<AppEvent>` (already available as `self.event_tx`) and spawn via `tokio::spawn` inside the already-running tokio context — or expose a method on `AsyncRuntime` and hold a reference in `App`.

### Warnings (SHOULD FIX)

1. **`AiTitleEngineConfig` is defined with `#[allow(dead_code)]` and not wired into `ConfigFile`** — It exists as a standalone struct in `config.rs` with no field in `ConfigFile` that uses it. `ai_title.rs` takes `&AiTitleEngineConfig` as a parameter, but callers (when they eventually exist) would have to construct it independently rather than reading it from user config. This is a design gap that will require a follow-up fix once AI title generation is actually triggered.

2. **Status bar uses Unicode symbols instead of spec's Japanese labels** — The spec states the format `"実行中:N  完了:N  待機:N"`. The implementation uses `"⏵:N ✓:N ⏸:N"`. Additionally, the time display (`| 時刻`) is absent entirely. While the symbols are functionally equivalent, the deviation from the spec's exact format may matter for acceptance. The missing clock is a gap.

3. **`respect_terminal_bg` default is `true` in `StatusConfig`, which disables background color changes by default** — `config.rs:325` sets `respect_terminal_bg: true`, so `show_status_bg` in `ui.rs:371-372` evaluates to `false` unless the user explicitly sets `respect_terminal_bg = false` in config.toml. This means the Done/Waiting background colors (`#0d2b0d`, `#2b1a00`) are invisible in a default installation. The logic is correct per the constraint ("respect_terminal_bg=true must disable background color changes"), but the default being `true` effectively silences the feature for all users who haven't touched config.toml. Verify this matches the intended UX.

4. **`FeatureToggleState.pending` is initialized from `FeaturesConfig::default()` in `App::new`, not from `config.features`** — `app.rs:538-541` sets `pending: FeaturesConfig::default()`. When the user opens the `?` dialog, `pending` is then overwritten with `self.config.features.clone()` (line 799), so the dialog opens correctly. However, the initial value in `App::new` is wasteful and slightly misleading; it should be initialized consistently.

5. **`#[allow(dead_code)]` on `Waiting` variant and multiple functions** — `PaneStatus::Waiting` (line 35), `AiTitleEngineConfig` (line 258), `generate_title` (line 21 in ai_title.rs), `detect_prompt_return` (line 57), and both `invoke_claude_headless`/`invoke_ollama` (ai_invoke.rs) are all suppressed dead-code warnings. This is the compiler flagging that these code paths are genuinely unreachable, which corroborates blocking issue #3.

### Summary
- Blocking: 3 issues
- Warnings: 5 issues
- Verdict: REQUEST_CHANGES (3 blocking issues)
