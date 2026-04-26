# Synthesis — Iteration 1

## Overall Verdict: AUTO_FIX

All four reviewers returned REQUEST_CHANGES. There are 8 AUTO_FIX items and 2 ASK_USER items that must be resolved before the iteration can be declared done.

---

## Issue Table

| ID | Issue | Reviewers | Decision | Priority |
|----|-------|-----------|----------|----------|
| F1 | reqwest added in violation of ureq constraint | alignment | AUTO_FIX | CRITICAL |
| F2 | AI title generation pipeline never wired — feature is decorative | quality, alignment | AUTO_FIX | CRITICAL |
| F3 | Shell/prompt injection via unsanitized pane output in CLI arg | security, robustness | AUTO_FIX | CRITICAL |
| F4 | bind_with_retry removes socket on any error, wrong errno (48 vs 98) | security, robustness, quality | AUTO_FIX | HIGH |
| F5 | Unix socket has no access-control permissions (world-readable) | security | AUTO_FIX | HIGH |
| F6 | pane_id not validated against real panes; arbitrary IDs create phantom state | security, robustness | AUTO_FIX | HIGH |
| F7 | AsyncRuntime drops blocking on infinite hook accept loop — process hangs on exit | robustness | AUTO_FIX | HIGH |
| F8 | pane_states / pane_output_rings / ai_titles never cleaned up on pane close | security, robustness | AUTO_FIX | MEDIUM |
| U1 | PaneStatus::Waiting never set; "notification" hook falls through | alignment | ASK_USER | HIGH |
| U2 | Ctrl+A and ? dialog are desynchronized (two sources of truth for ai_title_enabled) | quality | ASK_USER | MEDIUM |
| P1 | reqwest::Client created per request (no pooling) | security, robustness | PASS (subsumed by F1) | — |
| P2 | accept() errors silently swallowed with continue | robustness | PASS | LOW |
| P3 | Mutex poisoning recovery silently masks underlying panic | robustness | PASS | LOW |
| P4 | ai_title.rs prompt hardcoded in Japanese | quality | PASS | LOW |
| P5 | bind_with_retry declared async with no await points | robustness | PASS | LOW |
| P6 | max_chars + 5 tolerance window undocumented | robustness | PASS | LOW |
| P7 | &PathBuf parameter should be &Path | quality | PASS | LOW |
| P8 | status bar symbols differ from spec Japanese labels | alignment | PASS | LOW |
| P9 | respect_terminal_bg defaults true (silences bg color feature by default) | alignment | PASS (by design) | — |
| P10 | Section-divider comments (AI-generated noise) | quality | PASS | LOW |
| P11 | get_by_key/set_by_key don't cover all FeaturesConfig fields | robustness, quality | PASS | LOW |
| P12 | worker_threads(2) may starve hook accept loop | security | PASS | LOW |
| P13 | AiTitleEngineConfig not wired into ConfigFile | quality, alignment | PASS (subsumed by F2) | — |

---

## AUTO_FIX Items (for deep-implementer)

### F1: Remove reqwest — rewrite Ollama call with ureq + spawn_blocking
**File**: `Cargo.toml:28`, `src/ai_invoke.rs:50-65`
**Problem**: `reqwest` was added in violation of the explicit constraint "ureq must NOT be replaced with reqwest". It duplicates the existing `ureq` dependency and pulls in an async HTTP stack unnecessarily.
**Fix**: Remove `reqwest` from `Cargo.toml`. Rewrite `run_ollama` using a `tokio::task::spawn_blocking` closure that calls `ureq::post(...).send_json(...)` synchronously. The `ureq` crate is already in the dependency tree.

---

### F2: Wire the AI title generation pipeline end-to-end
**Files**: `src/app.rs` (drain_pty_events, ~line 2261), `src/ai_title.rs`, `src/ai_invoke.rs`
**Problem**: `generate_title`, `detect_prompt_return`, `invoke_claude_headless`, and `invoke_ollama` are all defined but never called. `AppEvent::AiTitleGenerated` is only handled on the receive side; no task ever sends it. `Ctrl+A` toggle and the status bar indicator exist, but the feature does nothing. `pane_output_rings` is populated with no consumer. All live code paths carry `#[allow(dead_code)]` suppressions as a tell.
**Fix**:
1. In `drain_pty_events`, after pushing to `pane_output_rings`, check `if self.ai_title_enabled` and call `ai_title::detect_prompt_return` on the last ring line.
2. When `detect_prompt_return` returns true, spawn a task via `async_runtime` (or `std::thread::spawn`) to call `generate_title` and send `AppEvent::AiTitleGenerated { pane_id, title }` back through `event_tx`.
3. Remove all `#[allow(dead_code)]` annotations from `ai_title.rs` and `ai_invoke.rs` once the pipeline is live.
4. Ensure `cargo build --release` and `cargo clippy -- -D warnings` both pass with zero warnings.

---

### F3: Sanitize and truncate pane output before embedding in AI prompt
**Files**: `src/ai_invoke.rs:16-18`, `src/ai_title.rs:31-34`
**Problem**: `run_claude_headless` passes raw PTY screen content as a positional CLI argument to `claude --print <prompt>`. The pane output is attacker-controlled (any program running in the pane controls it). Content starting with `--` can be misinterpreted as flags. An adversarial pane process could craft output to perform prompt injection. Additionally, 50 lines × up to 512 bytes each ≈ 25 KB can exceed OS `ARG_MAX` limits (~2 MB on Linux), causing silent `Command` failures for large buffers.
**Fix**:
1. In `ai_title.rs::generate_title`, truncate `pane_output` to 2 000 characters maximum before building the prompt string.
2. Strip or escape shell-special bytes (`$`, `` ` ``, `\`, NUL) from the pane content portion of the prompt.
3. Prefer passing the prompt via stdin rather than as a CLI argument: use `.stdin(Stdio::piped())` and write the prompt bytes to stdin, invoking `claude --print -` (or equivalent). If the `claude` CLI does not support stdin input, write to a `NamedTempFile` with permissions `0o600` and pass the file path.

---

### F4: Fix bind_with_retry — use ErrorKind::AddrInUse, add liveness probe
**File**: `src/hooks.rs:82-96`
**Problem**: The errno-48 branch is macOS/BSD-only (Linux uses 98); the fallthrough `Err(_)` arm already covers it identically, making the specific branch dead code on Linux. More dangerously, any bind error — including `EACCES`, `ENOENT` on parent directory, or a read-only filesystem — triggers an unconditional `remove_file` + retry, silently swallowing unrecoverable errors and potentially removing sockets owned by other processes.
**Fix**:
1. Replace `e.raw_os_error() == Some(48)` with `e.kind() == std::io::ErrorKind::AddrInUse` (cross-platform).
2. Before removing the stale socket, attempt `std::os::unix::net::UnixStream::connect(&path)`. If connect succeeds, a live process owns the socket — do not remove it; return an error instead. Only remove if connect fails (process is dead).
3. Remove the duplicate `Err(_)` catch-all retry arm; propagate other error kinds as failures rather than silently retrying.

---

### F5: Restrict Unix socket permissions to owner-only (0o600)
**File**: `src/hooks.rs:49` (after `UnixListener::bind`)
**Problem**: The socket at `~/.config/glowmux/hooks.sock` is created with the process umask applied, typically resulting in world-readable/executable permissions (`0o755`). Any local user can connect and inject arbitrary `HookReceived` events, including falsely marking panes Done/Running or crafting pane IDs that target another user's session.
**Fix**: Immediately after `UnixListener::bind(&socket_path)` succeeds, call:
```rust
#[cfg(unix)]
std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
```
Gate behind `#[cfg(unix)]` so the code compiles on all platforms.

---

### F6: Validate pane_id against real panes before acting on HookReceived
**Files**: `src/app.rs:2292-2308`, `src/hooks.rs:116`
**Problem**: `msg.pane_id.unwrap_or(0)` silently maps a missing `pane_id` to 0, creating a phantom `PaneState` for a non-existent pane. Any numeric `pane_id` sent by a client unconditionally creates a new entry via `pane_states.entry(id).or_default()`, enabling unbounded `HashMap` growth and state injection for arbitrary pane IDs. Pane IDs start at 1, so the default-to-0 fallback additionally poisons a phantom slot.
**Fix**:
1. In `handle_connection` (`hooks.rs`), return early with a log warning when `pane_id` is `None` for events that require one (`Stop`, `UserPromptSubmit`, `PreToolUse`). For `Notification`, decide explicitly whether to broadcast or discard — do not default to 0.
2. In the `AppEvent::HookReceived` handler (`app.rs`), before inserting into `pane_states`, verify the `pane_id` exists in `self.workspaces[..].panes`. Reject unknown IDs silently (no insert, no state change).

---

### F7: Fix AsyncRuntime shutdown — prevent process hang on exit
**File**: `src/runtime.rs:20`
**Problem**: `self.runtime.spawn(future)` discards the `JoinHandle`. The hook server runs an infinite `loop` with no cancellation token. When `AsyncRuntime` drops at the end of `main()`, `Runtime::drop` blocks waiting for all spawned tasks to complete, causing the process to hang forever after the TUI has been torn down.
**Fix**: Add an explicit `Drop` impl on `AsyncRuntime` that calls `self.runtime.shutdown_background()` instead of the default blocking drop. This drops the runtime without waiting for tasks, giving a clean exit:
```rust
impl Drop for AsyncRuntime {
    fn drop(&mut self) {
        // shutdown_background drops without waiting for tasks to finish,
        // preventing a hang on the infinite hook accept loop.
        // Safety: tasks are non-critical background I/O; losing in-flight
        // accepts on shutdown is acceptable.
        self.runtime.shutdown_background();
    }
}
```

---

### F8: Clean up pane_states / pane_output_rings / ai_titles when a pane is closed
**Files**: `src/app.rs` (close_pane path, ~line 1219)
**Problem**: `close_pane` removes the pane from `ws.panes` and calls `claude_monitor.remove`, but the three new `HashMap`s introduced in this iteration — `pane_states`, `pane_output_rings`, and `ai_titles` — are never pruned. In a long session with many splits and closes, these maps grow monotonically without bound.
**Fix**: In the `close_pane` path, add cleanup for all three maps:
```rust
self.pane_states.remove(&focused);
self.pane_output_rings.remove(&focused);
self.ai_titles.remove(&focused);
```

---

## ASK_USER Items (pause for user)

### U1: PaneStatus::Waiting — which hook event should set it?
**Reviewers**: alignment
**Problem**: `PaneStatus::Waiting` is defined and rendered (yellow dot, background color) but the `HookReceived` handler in `app.rs:2292-2308` only sets `Running` (on `user_prompt_submit`/`pre_tool_use`) and `Done` (on `stop`). The `"notification"` event falls through the `_ => {}` arm with no state change. The alignment reviewer proposes `"notification" => Waiting`, but it is not entirely clear from the spec whether every notification should cause the Waiting state, or only specific notification subtypes.
**Options**:
  - A) Map `"notification"` hook event → `PaneStatus::Waiting` unconditionally (simplest, matches reviewer suggestion).
  - B) Only set `Waiting` for notification payloads that contain a specific field (e.g., `"type": "waiting"`) — requires a richer `HookMessage` schema.
  - C) Leave `Waiting` unset for now and remove the variant + UI rendering until a clear trigger is specified.

---

### U2: Single source of truth for ai_title enabled state
**Reviewers**: quality
**Problem**: `Ctrl+A` toggles `app.ai_title_enabled` (an ephemeral bool on `App`), while the `?` feature-toggle dialog reads/writes `app.config.features.ai_title`. The status bar reads `app.ai_title_enabled`; the dialog renders from `feature_toggle.pending` (cloned from `config.features` on open). A `Ctrl+A` toggle is invisible to the dialog and vice versa. The two mechanisms are desynchronized.
**Options**:
  - A) Remove `app.ai_title_enabled`; use `self.config.features.ai_title` as the single source of truth everywhere (recommended by reviewer).
  - B) Keep `app.ai_title_enabled` as a runtime cache, but sync it to `config.features.ai_title` on every `Ctrl+A` press and on dialog confirm.
  - C) Keep `Ctrl+A` as a session-only override (does not persist) and have the dialog persist to config — but document this distinction explicitly in the UI.

---

## PASS Items

The following issues were raised by a single reviewer at low/informational severity and require no immediate action:

- **P1** reqwest::Client per-call (no pooling) — subsumed by F1 (reqwest being removed entirely).
- **P2** `accept()` errors silently swallowed with `continue` — acceptable for now; add counter in future hardening pass.
- **P3** Mutex poisoning recovery silently masks panic — low risk; add a tracing log in a future quality pass.
- **P4** Prompt hardcoded in Japanese — belongs in `AiTitleConfig.prompt`; defer until AI pipeline is live (F2).
- **P5** `bind_with_retry` declared `async` with no `.await` points — cosmetic; fix as part of F4 cleanup.
- **P6** `max_chars + 5` tolerance window undocumented — add comment; not blocking.
- **P7** `&PathBuf` parameter should be `&Path` — minor API guideline violation; fix opportunistically.
- **P8** Status bar symbols differ from spec Japanese labels — cosmetic deviation; acceptable unless spec is treated as pixel-perfect.
- **P9** `respect_terminal_bg` defaults `true` silencing bg color — confirmed intentional per constraint; no change needed.
- **P10** Section-divider AI-generated comments — cosmetic; out of scope for this iteration.
- **P11** `get_by_key`/`set_by_key` don't cover all `FeaturesConfig` fields — the unincluded fields are intentionally startup-only; add a comment documenting the division.
- **P12** `worker_threads(2)` may starve hook accept loop — acceptable for now; revisit if performance issues arise.
- **P13** `AiTitleEngineConfig` not wired into `ConfigFile` — subsumed by F2; wire it when the AI pipeline is completed.
