## Review Result: REQUEST_CHANGES

### Blocking Issues (MUST FIX)

1. **[CRITICAL] src/ai_invoke.rs:17 — Shell injection via unsanitized prompt argument**

   `Command::new("claude").args(["--print", prompt])` passes the raw prompt as a single
   argument. On most shells and `exec`-family calls this is safe from shell injection, but
   the `claude` CLI may itself interpret special characters in the prompt string (e.g.
   backticks, `$(...)`, glob patterns) depending on how it parses its own argv. More
   importantly, the prompt is constructed in `ai_title.rs` by directly embedding raw PTY
   screen content (`pane_output`) into a format string with no escaping or length cap
   before it reaches `invoke_claude_headless`. An adversarial program running inside a
   pane could craft its output to inject content into the prompt that manipulates the
   headless `claude` invocation (prompt injection). The 65 536-byte socket read limit does
   not apply here because `pane_output` comes from the in-process VTE parser, not the
   socket.

   Fix: Sanitize / truncate `pane_output` before building the prompt. Strip or escape
   shell-special bytes (`$`, `` ` ``, `\`, NUL) from the pane content slice. Apply a
   hard character budget (e.g. 2 000 chars) on the pane content portion alone, separate
   from the `max_chars` limit on the title output.

2. **[CRITICAL] src/hooks.rs:90 — bind_with_retry removes the socket unconditionally on any bind error**

   The second `Err(_)` arm calls `fs::remove_file(path)` and retries for every bind
   error, including permission-denied, read-only filesystem, and path-not-a-socket
   errors. On a permission-denied filesystem this silently swallows the error and returns
   `None` only after the retry also fails — which is acceptable. The real danger is a
   TOCTOU race: if two glowmux instances start simultaneously both can enter the stale-
   socket removal path, both call `remove_file`, one succeeds and one gets ENOENT, then
   both retry `bind`, and one wins. The loser's `HookServerGuard` is never created
   (because `bind_with_retry` returned `None`), so the socket file that the winner
   created is never cleaned up on abnormal exit either — the guard is only constructed in
   `start_hook_server` after a successful bind. Additionally, `EADDRINUSE` on Linux is
   errno 98, not 48. The `Some(48)` branch is macOS/BSD-only; on Linux the fallthrough
   `Err(_)` arm handles it, which is functionally correct but implies the errno-specific
   branch is dead code on Linux and misleads readers.

   Fix: Remove the `e.raw_os_error() == Some(48)` branch entirely (the generic `Err(_)`
   arm already covers it). Add a liveness probe before removing the stale socket (attempt
   `UnixStream::connect` first; remove only if connect fails). Add a comment documenting
   the remaining race window.

3. **[CRITICAL] src/runtime.rs:20 — JoinHandle discarded; no task cancellation on shutdown**

   `self.runtime.spawn(future)` discards the returned `JoinHandle`. When `AsyncRuntime`
   drops at end of `main()`, the Tokio runtime calls `Runtime::drop`, which blocks until
   all spawned tasks complete. `start_hook_server` contains an infinite `loop` with no
   cancellation token or select on a shutdown signal, so the runtime will block forever
   at shutdown waiting for that task. On a clean Ctrl+Q path this means the process hangs
   after the TUI has already been torn down.

   Fix: Either (a) use `runtime.shutdown_background()` in `AsyncRuntime::drop` to drop
   the runtime without waiting for tasks, or (b) thread a `CancellationToken` (from
   `tokio_util`) through `start_hook_server` and `select!` on it inside the accept loop,
   storing the `JoinHandle` for a graceful drain. Option (a) is simpler for the current
   architecture.

4. **[CRITICAL] src/hooks.rs:116 — pane_id defaults to 0 when absent; event silently applied to pane 0**

   `msg.pane_id.unwrap_or(0)` silently maps a missing `pane_id` field to pane ID 0. In
   all multi-pane workspaces pane 0 does not exist (IDs start at 1), so
   `pane_states.entry(0).or_default()` will create a phantom state entry for a
   non-existent pane. Hooks fired without a `pane_id` (e.g. global notification events)
   will silently poison the `pane_states` map. If a pane with ID 0 is ever created
   (edge-case startup path), this becomes a real status mismatch.

   Fix: Return early from `handle_connection` when `pane_id` is `None` and the event
   requires a pane (Stop, UserPromptSubmit, PreToolUse). For Notification, decide whether
   a broadcast or discard is correct, and implement that explicitly instead of defaulting
   to 0.

5. **[CRITICAL] src/ai_invoke.rs:59 — reqwest::Client created per request; no connection pooling or timeout**

   `run_ollama` creates a new `reqwest::Client` on every call. The `Client` struct
   encapsulates a connection pool; recreating it per request defeats the pool and leaks
   file descriptors on high-frequency title updates. Additionally, the HTTP request itself
   has no timeout; the outer `tokio::time::timeout` wrapping `run_ollama` in
   `invoke_ollama` covers the entire future, but if the Ollama server accepts the TCP
   connection and then stalls sending the response body, the timeout will still fire —
   however this only works correctly because the outer timeout wraps the full call.
   The deeper problem is the per-call `Client` allocation: each call opens a new TCP
   connection, which can exhaust ephemeral ports under high load.

   Fix: Make `Client` a `once_cell::sync::Lazy<reqwest::Client>` or pass it as a
   parameter from a shared location. A `static OLLAMA_CLIENT: Lazy<Client>` in
   `ai_invoke.rs` is the simplest fix.

---

### Warnings (SHOULD FIX)

1. **src/hooks.rs:70-73 — accept() errors silently swallowed with `continue`**

   `Err(_) => continue` on `listener.accept()` discards all I/O errors including
   permanent ones (e.g. too many open files, listener invalidated). If the underlying
   socket becomes unrecoverable the server will spin-loop at 100% CPU producing no
   output. Add an error counter and break/return after N consecutive failures.

2. **src/app.rs:2265 — Mutex poisoning recovery in event loop**

   `pane.parser.lock().unwrap_or_else(|e| e.into_inner())` recovers from a poisoned
   mutex by taking the inner value. This is intentional but silently masks the underlying
   panic that caused the poison. The recovered data may be in an inconsistent state.
   Consider logging a warning when poison is detected so it is not invisible in
   production.

3. **src/app.rs:2262 — `pane_output_rings` grows unbounded for closed panes**

   `pane_output_rings.entry(pane_id).or_default()` creates entries for every pane ID
   that ever produces output, including panes that have since been closed. There is no
   cleanup of dead pane entries. Over a long session with many splits/closes this map
   will grow monotonically.

   Fix: Remove the entry in `drain_pty_events`'s `PtyEof` handler, or prune entries
   whose `pane_id` is not present in any workspace.

4. **src/ai_title.rs:49 — max_chars overflow check is `max_chars + 5`, truncates to `max_chars`**

   The guard `trimmed.chars().count() > config.max_chars + 5` allows responses up to
   `max_chars + 5` characters through without truncation. This is a deliberate tolerance
   window, but it is undocumented and the asymmetry (allow +5, truncate to max_chars)
   could confuse future maintainers. At minimum add a comment explaining the intent.

5. **src/hooks.rs:82 — `bind_with_retry` is declared `async` but contains no `.await` points**

   The function is `async` but the only async operation (`UnixListener::bind`) is
   actually synchronous in tokio's Unix socket implementation. Mark it as a plain `fn` to
   avoid misleading readers and prevent accidental blocking-in-async assumptions. This is
   a correctness-cleanliness issue, not a runtime bug, because the caller is already in
   an async context.

6. **src/config.rs:50-59 — `get_by_key` and `set_by_key` do not cover all FeaturesConfig fields**

   `ai_worktree_name`, `auto_worktree`, `session_restore`, and `feature_toggle_ui` are
   defined in `FeaturesConfig` but are absent from `get_by_key`/`set_by_key`. The
   feature toggle UI silently ignores any attempt to toggle these features. This is
   probably intentional (they are not in `FEATURES`), but the gap between the config
   struct and the toggle-able set is a latent maintenance hazard.

7. **src/runtime.rs — `AsyncRuntime` has no `Default` impl and no `Drop` impl; shutdown behavior is implicit**

   The runtime will block on drop (see Blocking Issue 3). Even if shutdown_background is
   used, there is no explicit `Drop` impl documenting this behavior. Add an explicit
   `Drop` that calls `self.runtime.shutdown_background()` so the contract is clear.

---

### Summary

- Blocking: 5 issues
- Warnings: 7 issues
- Verdict: REQUEST_CHANGES (5 blocking issues)
