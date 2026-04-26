## Review Result: REQUEST_CHANGES

### Blocking Issues (MUST FIX)

1. [CRITICAL] src/ai_invoke.rs:16-18 — **Unsanitized user-controlled input passed as a CLI argument to `claude --print <prompt>`**

   `run_claude_headless` builds the prompt from `pane_output` (terminal screen content controlled by whatever program runs in the pane) and passes it verbatim as a positional argument via `.args(["--print", prompt])`. On most shells and the `claude` CLI, a prompt that begins with `--` or contains option-like tokens could be misinterpreted as flags, potentially triggering unintended CLI behaviour or flag injection. More critically, if the `claude` binary ever evaluates shell metacharacters, or if the calling code is ever extended to use `Command::new("sh").arg("-c")`, this becomes a full shell injection sink.

   Fix: Pass the prompt via stdin instead of as a CLI argument. Use `.arg("--print").stdin(Stdio::piped())`, write the prompt to stdin, and set `--print -` (or the equivalent flag for stdin input). If the CLI does not support stdin, write the prompt to a `NamedTempFile` with permissions `0o600` and pass the file path. Never interpolate user/pane content as a raw positional argument.

2. [HIGH] src/hooks.rs:82-96 — **Unconditional stale-socket removal allows local privilege escalation / denial of service**

   `bind_with_retry` removes the socket file on *any* bind error (the second `Err(_)` arm), not only `EADDRINUSE`. If glowmux is running as a normal user and the socket path is inside a world-writable temp directory (not the case with `dirs::config_dir`, but worth noting), or if a bind error is caused by a permission problem on a socket owned by a different process, the code silently removes that process's socket and tries to re-create it. More concretely, the two `Err` arms are identical — any bind error triggers a delete-and-retry, including `EACCES`, `ENOENT` (on a race), etc. There is no check that the existing socket is owned by the current user before deleting it.

   Fix: Only remove the socket file when the error is `EADDRINUSE` (errno 48 on macOS / 98 on Linux) **and** after verifying the path is not actively listened to (attempt a test connect; if it fails, the process is dead and the stale socket can safely be removed). Collapse the duplicate `Err(_)` arm — currently both arms do exactly the same thing, making the `e.raw_os_error() == Some(48)` match dead code.

3. [HIGH] src/hooks.rs:49 / src/app.rs:2292-2308 — **Unix socket accepts connections from any local user; no access control**

   The socket at `~/.config/glowmux/hooks.sock` is created without explicit permission bits. On Linux `UnixListener::bind` creates the socket with the process umask applied, which is typically `0o755` (world-readable/executable). Any local user can connect to the socket and inject arbitrary `HookReceived` events (e.g., sending `{"event":"stop","pane_id":0}` to falsely mark panes Done, or `{"event":"user_prompt_submit","pane_id":0}` to mark them Running), and can craft a `pane_id` value that targets panes belonging to the victim.

   Fix: After binding, call `std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))` (Unix only, gated behind `#[cfg(unix)]`). This restricts connections to the socket owner only.

4. [HIGH] src/hooks.rs:116 — **Arbitrary `pane_id` injection via socket message; no validation against real pane IDs**

   The deserialized `msg.pane_id` is used directly to key into `self.pane_states` (an `entry().or_default()` in `app.rs:2293`), unconditionally creating a new `PaneState` for any numeric ID sent by the client. A malicious local process (or the `EADDRINUSE` race above) can pre-populate `pane_states` with thousands of fake IDs before glowmux creates real panes, causing unbounded HashMap growth. It can also flip the state of any pane it knows the ID of.

   Fix: In `AppEvent::HookReceived` handling in `app.rs`, validate that `pane_id` corresponds to an existing pane across all workspaces before acting on or inserting into `pane_states`. Reject unknown IDs silently.

---

### Warnings (SHOULD FIX)

1. src/ai_invoke.rs:59 — **`reqwest::Client` created on every `invoke_ollama` call**

   A new HTTP client (with its own connection pool, TLS context, and thread-pool references) is allocated per invocation. For a long-running TUI this causes connection pool churn and unbounded resource allocation if title generation fires frequently.

   Fix: Store a `reqwest::Client` as a field in `AsyncRuntime` or as a `once_cell::sync::Lazy<Client>` static and reuse it.

2. src/hooks.rs:99-103 — **`read_to_end` with a 64 KiB limit is still generous for a hook message**

   A hook event payload is at most a small JSON object (< 256 bytes). A 64 KiB limit means a misbehaving client can hold a connection open while slowly sending 65 536 bytes of garbage, keeping a tokio task alive for the full read duration. With many concurrent connections this could exhaust the thread pool.

   Fix: Reduce the limit to 4 096 bytes (one page). Also add a read timeout via `tokio::time::timeout` wrapping the `read_to_end` call.

3. src/app.rs:2262 / close_pane at line 1219 — **`pane_states`, `pane_output_rings`, and `ai_titles` are never cleaned up when a pane is closed**

   `close_pane` removes the entry from `ws.panes` and calls `claude_monitor.remove(focused)` but the three new `HashMap`s introduced in this iteration — `pane_states`, `pane_output_rings`, `ai_titles` — are not pruned. Over a long session with many panes opened and closed, these maps grow without bound.

   Fix: In the `close_pane` path (around `app.rs:1219`), add:
   ```rust
   self.pane_states.remove(&focused);
   self.pane_output_rings.remove(&focused);
   self.ai_titles.remove(&focused);
   ```

4. src/ai_title.rs:31-34 — **Unbounded pane output embedded in prompt; no length cap before sending to AI**

   `generate_title` takes `pane_output` and embeds it verbatim in the prompt string. The ring buffer caps at 50 lines, but each line can be up to the terminal width (e.g., 512 bytes with wide unicode). At 50 × 512 = ~25 KB per request, prompt tokens are not bounded before network transmission, which can cause unexpectedly large requests and cost blowup with cloud backends.

   Fix: Truncate `pane_output` to a sensible max (e.g., 2 048 chars) before constructing the prompt.

5. src/ai_invoke.rs:50-52 — **Ollama `url` parameter not validated; SSRF to internal network possible**

   The Ollama base URL comes from user config and is joined with `/api/generate` without any validation. A malicious config entry of `http://169.254.169.254/latest/meta-data` (AWS IMDSv1) or `http://internal-service:8080` would be contacted by the `reqwest` client.

   Fix: Parse the URL with `reqwest::Url::parse`, then assert the scheme is `http` or `https` and the host is not a link-local, loopback (unless explicitly `localhost`/`127.0.0.1`), or private-range address — or at minimum document the risk and require explicit user acknowledgement in config.

6. src/hooks.rs:85 — **`raw_os_error() == Some(48)` is macOS/BSD-only; silently wrong on Linux**

   `EADDRINUSE` is errno 98 on Linux, not 48. The first match arm therefore never fires on Linux, falling through to the identical second `Err(_)` arm. This is functionally equivalent today (both arms do the same thing) but masks the intent and will silently misfire if the two arms are ever given different behaviour.

   Fix: Use `std::io::ErrorKind::AddrInUse` instead of raw OS errno:
   ```rust
   Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => { ... }
   ```

7. src/runtime.rs:9 — **`enable_all()` activates both `io` and `time` drivers unconditionally**

   `enable_all()` is fine here but the `worker_threads(2)` limit means all async work (hook accepts, AI calls, future features) shares two threads. A slow AI invocation that does not yield will starve the hook accept loop.

   Fix: Consider `worker_threads(4)` or make AI calls explicitly yield-aware with `spawn_blocking` for the subprocess wait. Document the thread budget.

---

### Summary

- Blocking: 4 issues
- Warnings: 7 issues
- Verdict: REQUEST_CHANGES
