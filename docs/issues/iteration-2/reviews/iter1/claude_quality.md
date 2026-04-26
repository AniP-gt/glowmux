## Review Result: REQUEST_CHANGES

### Blocking Issues (MUST FIX)

1. **[CORRECTNESS/DEAD CODE]** `src/ai_invoke.rs` entire file, `src/ai_title.rs` entire file — Both files are decorated `#[allow(dead_code)]` on every pub item, and neither is called from any live code path. `ai_title.rs::generate_title` is never invoked; `ai_invoke.rs::invoke_claude_headless` and `invoke_ollama` are never called outside of `ai_title.rs`. The feature described as "AI title generation" has no wiring: nothing triggers `generate_title`, no `AsyncRuntime::spawn` call initiates it, and `AppEvent::AiTitleGenerated` is only inserted into `ai_titles` on the receive side but never fired from any spawned task. The feature is advertised in the UI (`app.ai_title_enabled`, `Ctrl+A` toggle, `AI:on/off` in status bar) but does nothing. Shipping a toggle for a feature with zero implementation is misleading and constitutes dead-code bloat.
   Fix: Either wire the generation pipeline (spawn a task from `drain_pty_events` or a timer, send `AiTitleGenerated` events) so the feature actually functions, or remove `ai_invoke.rs`, `ai_title.rs`, `app.ai_title_enabled`, `App::ai_titles`, `App::pane_output_rings`, `App::last_ai_title_request`, and the `Ctrl+A` key binding until the iteration is complete.

2. **[CORRECTNESS]** `src/hooks.rs:85` — `bind_with_retry` hard-codes macOS/BSD errno 48 (`EADDRINUSE`) as the retry condition, then the `Err(_)` arm also removes the socket file and retries unconditionally. This means the function always silently swallows any `bind` error and retries regardless of whether it is `EADDRINUSE` or something unrecoverable (e.g. `EACCES`, `ENOENT` on parent dir). The comment acknowledges Linux uses errno 98 but the guard only names 48. The `Err(_)` fallthrough then makes the errno-48 branch entirely redundant — both arms do the same thing.
   Fix: Use `std::io::ErrorKind::AddrInUse` (which crossplatform-maps to both 48 and 98) as the retry condition. Remove the `Err(_)` catch-all retry; instead propagate other errors so the server does not silently fail on permission errors.

3. **[CORRECTNESS]** `src/hooks.rs:118-128` — After parsing `HookMessage`, `HookEvent::from_str` is called to parse `msg.event`, and then the matched `HookEvent` variant is immediately converted back to the same string literal and sent as `AppEvent::HookReceived { event: event_name.to_string() }`. The round-trip parse→stringify is pure overhead: the receiver in `drain_pty_events` does a second string match (`event.as_str()`) on the same string. The typed `HookEvent` enum exists but is never used as the payload type.
   Fix: Send `AppEvent::HookReceived { pane_id, event: HookEvent }` directly (change the enum variant to carry `HookEvent` instead of `String`), eliminating both the round-trip stringify and the second string match in `drain_pty_events`.

4. **[DEAD CODE / CONFIG INTEGRITY]** `src/config.rs:261` — `AiTitleEngineConfig` is defined with `#[allow(dead_code)]` and its `Default` impl exists, but it is not a field of `ConfigFile`, `AiConfig`, or any loaded struct. It is unreachable from the config loading path. `ai_title.rs::generate_title` accepts it as a parameter, but as noted in blocking issue 1, `generate_title` is never called. This struct cannot be configured by users and has no effect at runtime.
   Fix: Either wire `AiTitleEngineConfig` into `AiConfig` (add a field `engine: AiTitleEngineConfig`) so it can be loaded from `config.toml`, or delete it until the pipeline is implemented.

5. **[CORRECTNESS / PERFORMANCE]** `src/app.rs:2261-2291` — `AppEvent::PtyOutput` handler reads the entire last screen row for every PTY event, for all panes across all workspaces, on every event tick. This runs a `Mutex::lock` inside the event drain loop, allocates a `String` per event, and pushes to `pane_output_rings` regardless of whether `ai_title_enabled` is true or whether any consumer will ever read the ring. Since AI title generation is not wired (blocking issue 1), this entire code block executes only to populate a ring buffer that is never consumed.
   Fix: Guard the ring-building block with `if self.ai_title_enabled` at minimum. When the AI pipeline is implemented, this should only run for the relevant pane (not all panes on every event) and should sample less frequently (use a debounce timer, not every `PtyOutput` event).

---

### Warnings (SHOULD FIX)

1. `src/runtime.rs:20` — `AsyncRuntime::spawn` discards the returned `JoinHandle`. If the spawned future panics, the error is silently swallowed and the runtime continues. For the hook server future specifically, a panic would silently kill the Unix socket listener.
   Suggestion: Store and periodically check the `JoinHandle`, or at minimum wrap the future body in `std::panic::catch_unwind`-equivalent logic.

2. `src/hooks.rs:49` — `start_hook_server` takes `tx: Sender<AppEvent>` where `Sender` is the stdlib `std::sync::mpsc::Sender`, but this function is `async` and runs inside a tokio runtime. Calling `tx.send()` from within an async task is fine for mpsc, but mixing tokio tasks with stdlib blocking channel sends is a subtle layering violation. If the channel becomes full (which std mpsc unbounded cannot, but a future refactor could change), the send would block a tokio worker thread.
   Suggestion: Document the intentional use of `std::sync::mpsc::Sender` inside a tokio task, or switch to `tokio::sync::mpsc` for consistency with the async runtime.

3. `src/ai_invoke.rs:15-31` — `run_claude_headless` passes the prompt as a CLI argument directly: `.args(["--print", prompt])`. Shell argument injection is not possible here (no shell is invoked; args are passed as a `Vec`), but the prompt is user-visible terminal output content which could be arbitrarily long. There is no length limit before the `timeout` wrapper, meaning a very large pane buffer could produce a multi-megabyte argument. Some OS argument length limits (ARG_MAX ~2MB on Linux) would cause the `Command` to silently fail.
   Suggestion: Add a pre-check to truncate the prompt to a safe length (e.g. 8 KB) before passing to the subprocess.

4. `src/app.rs:784-790` — `Ctrl+A` toggles `app.ai_title_enabled` but does not sync this to `app.config.features.ai_title`. The feature toggle dialog (`?` key) reads and writes `config.features.ai_title` via `pending`/`get_by_key`. These two toggle mechanisms are desynchronized: `Ctrl+A` changes the ephemeral `ai_title_enabled` bool while the dialog changes `config.features.ai_title`. The status bar indicator reads `app.ai_title_enabled`, but dialog rendering reads `app.feature_toggle.pending.get_by_key("ai_title")`. On dialog open, `pending` is cloned from `config.features`, so `Ctrl+A` changes will be invisible to the dialog unless `ai_title_enabled` is also reflected in `config.features.ai_title`.
   Fix: Remove the separate `ai_title_enabled` field; use `self.config.features.ai_title` as the single source of truth.

5. `src/config.rs:49-71` — `get_by_key` and `set_by_key` on `FeaturesConfig` cover only 5 of the 9 fields (`ai_title`, `status_dot`, `status_bg_color`, `status_bar`, `zoom`). Fields `ai_worktree_name`, `auto_worktree`, `session_restore`, and `feature_toggle_ui` are silently dropped by the `_ => false` / `_ => {}` fallbacks. `FEATURES` in `app.rs` also lists only 5 entries. This means four config fields are not toggleable at runtime, which may be intentional, but the asymmetry between the struct definition and the dispatch methods is undocumented and invites future bugs where someone adds a field to the struct but forgets the match arms.
   Suggestion: Add a comment in `FeaturesConfig` explicitly listing which fields are runtime-toggleable vs. startup-only, or assert in tests that `get_by_key`/`set_by_key` cover all `FEATURES` entries.

6. `src/hooks.rs:82` — `bind_with_retry` takes `path: &PathBuf` but `&PathBuf` should be `&Path`. Accepting `&PathBuf` prevents callers from passing `&Path` directly and is an unnecessary restriction per Rust API guidelines.

7. `src/ai_title.rs:30-35` — The prompt for title generation is hardcoded in Japanese: `"以下のターミナル出力を{}文字以内の日本語で要約してください。タイトルのみ返してください。\n{}"`. Language is hardcoded to Japanese regardless of user locale. This belongs in `AiTitleConfig.prompt` (which already has a placeholder field) rather than embedded in the function body.

8. **Comment quality** — Multiple instances of section-divider comments (`// ─── ...`) throughout `app.rs` and `ui.rs` are AI-generated visual noise. These are present in the existing codebase as well, but the new additions (`// ─── Layout Tree`, `// ─── Text Selection`, `// ─── Workspace`, etc.) continue the anti-pattern. They add no information that the function/struct names don't already communicate.

---

### Positive

- `HookServerGuard::drop` correctly removes the socket file on shutdown — proper RAII cleanup.
- `handle_connection` uses `take(65536)` to bound the read size, preventing unbounded memory allocation from a misbehaving client.
- `AsyncRuntime` correctly initializes the tokio runtime before raw mode in `main.rs`, avoiding the double-init pitfall.
- `FeaturesConfig` with `serde(default)` and separate `get_by_key`/`set_by_key` is the right shape for a runtime feature-flag system.
- The `?` dialog opens a `pending` copy of features instead of mutating live config directly — correct transactional semantics.
- `bind_with_retry` correctly removes a stale socket file before retrying bind, which is the standard pattern for Unix domain sockets.

---

### Summary

- Blocking: 5 issues
- Warnings: 8 issues
- Verdict: REQUEST_CHANGES
