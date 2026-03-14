# Local macOS Smoke Checklist

Use this checklist before calling a Relay build locally release-ready on macOS.

## Scope

- Target platform: macOS
- Primary goal: validate that the app launches, completes the core transfer flows, and behaves sanely when something goes wrong
- Companion checks: automated verification still runs from `.codex/verify.commands`

## Preflight

1. Start the local signaling server:

   ```bash
   cd server
   go build -o relay-server .
   ./relay-server
   ```

2. Verify the server is healthy:

   ```bash
   curl -sf http://127.0.0.1:8080/health
   ```

3. Launch the client:

   ```bash
   cd client
   pnpm install --frozen-lockfile
   pnpm tauri dev
   ```

4. If the app fails during startup, retry with Rust backtraces enabled:

   ```bash
   cd client
   RUST_BACKTRACE=1 pnpm tauri dev
   ```

## Must-pass flows

### 1. Boot + settings

Pass:
- App window opens without a blank screen or crash
- Home screen shows Send, Receive, and Settings
- Settings open and close cleanly
- Signaling server URL is visible and defaults to `ws://localhost:8080`
- Settings persist across restart

Fail:
- Crash on launch
- Unusable UI
- Settings do not persist

### 2. Single-file direct transfer

Pass:
- Sender picks a small file
- Receiver enters the code
- Offer appears and is accepted
- Transfer completes
- Completion summary matches expected file count and size
- File contents match the source
- Connection badge shows `Direct P2P`

Fail:
- Pairing stalls
- Wrong file size
- Corrupted output
- Wrong save location
- Missing completion state

### 3. Cancel in progress

Pass:
- Cancel during an active transfer returns both sides to a sane idle or recoverable state
- No crash
- No silent partial-file confusion remains

Fail:
- Hung UI
- Ghost session
- Crash
- Ambiguous partial-file state

### 4. Invalid or stale code

Pass:
- Malformed or expired code shows a clear recoverable error
- User can retry without restarting the app

Fail:
- Endless spinner
- Generic unrecoverable error
- App restart required

### 5. Multi-file or folder transfer

Pass:
- Nested visible files arrive intact
- Relative paths are preserved
- Counts and summary are correct

Fail:
- Dropped files
- Flattened folder structure
- Incorrect summary
- False-positive path security errors

### 6. Settings-driven save path

Pass:
- Custom save path is honored
- Save path persists across restart

Fail:
- Files save elsewhere
- Setting is ignored
- Permission failure is unclear

## Release-blocking manual validation

### 7. Relay fallback

This must be validated before a public release claim.

Pass:
- A real transfer completes when direct QUIC is unavailable
- UI shows `Relay`

Fail:
- Fallback never activates
- Relay transfer fails

Notes:
- Existing automated coverage in `client/src-tauri/tests/signaling_e2e.rs` is useful evidence, but it does not replace a real operator-facing run.
- If same-Mac two-window testing is unreliable, use a second machine or a separately packaged local build for the second peer.

## Evidence to capture

- Build version or branch
- Exact command used to launch server and client
- Whether the run used `Direct P2P` or `Relay`
- Screenshot of the main screen and completion screen
- Any error text shown in the UI
- Log snippet for failures

## Recommended order

1. Boot + settings
2. Single-file direct transfer
3. Cancel in progress
4. Invalid or stale code
5. Multi-file or folder transfer
6. Settings-driven save path
7. Relay fallback
