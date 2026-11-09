# Temporary browser client

This is a separate browser entry point. The desktop Vite/Tauri app remains under
`src/`. The browser uses native WebRTC and the same Rust `ssc-protocol` WASM module
as the desktop protocol, including invitation codec, commitment/reveal, SAS,
READY, encryption, chunking and message reassembly. TypeScript supplies transport,
room HTTP requests and the interface; it does not implement the cryptography.

## Build and development

From the repository root, use `npm run web:build`. The root build script compiles
WASM with the pinned wasm-bindgen CLI, publishes its generated module and binary
under a content-addressed protocol path, typechecks, and builds `web/dist`.
Generated WASM, build outputs and test artifacts are ignored by Git. Serve the
whole output with its generated WASM files and `application/wasm` MIME type.
Retain old content-addressed protocol assets when switching HTML releases.

For development, build WASM into `web/public/protocol` with
`crates/ssc-protocol/build-wasm.sh`, then run `npm run web:dev`. Vite listens on
loopback port1430 and proxies room requests to `SSC_WEB_API_ORIGIN`, default
`http://127.0.0.1:8080`. The helper's public origin must match the browser origin.
`VITE_SSC_PROTOCOL_URL` selects a generated same-origin module path during build;
it is not a runtime URL supplied by a user or room response.

## Lifecycle

Opening `/r/ID#k=SECRET` displays a preview without claiming the guest slot.
The fragment is moved to memory and removed from the address/history entry.
Only clicking **Войти** submits the join request. A per-opening claim key remains
in memory for safe join retries. Creation retries reuse an in-memory idempotency
key. Creator and guest capabilities are bearer headers, never URL parameters.

Both participants explicitly compare and confirm the complete SAS before
messages become available. A visible countdown uses the fixed room deadline;
notifications appear below five minutes and below one minute. Deadlines are pinned
to monotonic time, bounded by the original join deadline plus one hour and by
one hour from transport connection. Repeated identical/later server deadlines
and wall-clock corrections cannot renew them; earlier server expiry can shorten them. There is no
automatic retry of chat messages. **Передано каналу** means accepted by the
local DataChannel, not delivery/read confirmation.

Explicit exit and `pagehide` clear the active transport, protocol state and UI
history and attempt a room DELETE (`keepalive` on page exit). These requests are
best effort when offline or when a browser is killed; server expiry independently
limits the room and TURN leases. Visibility changes alone do not end the chat.
A refresh cannot recover the session. Disconnection gets at most ten seconds to
recover; a new session is needed afterwards. The handshake has a twenty-second
transport deadline and human code comparison is not subject to that deadline.

No capabilities, keys, message history or draft are written to browser storage.
There are no service workers, third-party scripts, analytics or remote fonts.
JavaScript cannot guarantee erasure of all engine/browser/OS memory copies.
User-sent clipboard links and the browser/device itself remain outside that
application-level clearing guarantee. Incoming and outgoing message history is
bounded to128 messages and8MiB in application state.

## Checks

`npm run web:test` uses Playwright against a running web server and real helper.
Set `SSC_WEB_TEST_URL` for another origin. Tests use isolated contexts and no
network, RTC or cryptographic mocks. The Chromium loopback flag is only for the
local TURN fixture. Trace/video/screenshots and Playwright's automatic failure
DOM snapshot are disabled: DOM snapshots could contain invitation capabilities
or messages. Do not enable them for real sessions or upload sensitive artifacts.

The protocol browser test runs real WASM commit/reveal/readiness/chunking. The
room tests verify non-consuming preview, two-sided SAS, bidirectional Unicode,
no storage/fragment retention, both peers ending on explicit exit, clock rollback/forward stability and simulated
one-hour monotonic expiry. Set `SSC_WEB_BROWSER_MATRIX=1` to include Firefox and
WebKit; the default public CI gate is Chromium. WebKit automation is not physical
Safari/iOS acceptance. `SSC_WEB_FORCE_RELAY=1` is a test-only constructor harness
that forces the real RTC relay policy and asserts actual selected relay pairs. Local
acceptance is evidence for its tested browser/network only, not arbitrary NAT,
Safari/iOS, or external relay acceptance.
