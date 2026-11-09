# Desktop security maintenance

The current protocol is v3. Both participants must update; v2 invitations and handshake
frames are rejected. Manual invitation exchange and configurable STUN/TURN remain available.
The pairing helper is optional for manual exchange. It sees connection metadata, never chat
message encryption keys. A shared link is a capability: send it only to its intended recipient.
Always compare the entire SAS through a trusted channel before confirming.

## Dependencies

Commit both lockfiles. Use `npm ci`, `npm audit` and `cargo audit --file src-tauri/Cargo.lock`
when changing dependencies. The desktop-security workflow runs these alongside regression
checks on feature pushes and pull requests. The scoped minimatch override supplies the
patched 11.0.1 line to typescript-eslint until its upstream range moves forward; lint/tests
validate that integration. Do not use `npm audit fix --force` without reviewing the changes.

The original WebRTC ICE implementation only gathered UDP TURN candidates; accepting TCP/TLS
settings was not proof of working support. The vendored ICE transport patch adds framed
TURN-over-TCP and certificate-validated TLS, with independent stream tests and real relay checks.
See [ICE patch provenance](../src-tauri/vendor/webrtc-ice/SSC-PATCH.md).
Its DTLS dependency is vendored with a small
API migration to p256/p384 0.14 and patched crypto-bigint. No arithmetic is reimplemented;
the original library tests and additional curve agreement tests exercise the patch. See
[patch provenance](../src-tauri/vendor/webrtc-dtls/SSC-PATCH.md). This removes versions affected by [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).
The refreshed rustls/webpki chain includes the fix for
[distrusted intermediates](https://rustsec.org/advisories/RUSTSEC-2026-0382.html).

The Linux GUI stack still produces RustSec **warnings**, distinct from reported dependency
vulnerabilities: glib 0.18's VariantStrIter unsoundness and unmaintained bincode, pin-utils, proc-macro-error,
zbus_names, zvariant and zvariant_derive. Tauri's GTK dependency currently selects these
versions. Application code does not call VariantStrIter. These warnings are not suppressed;
a clean vulnerability scan does not certify that the underlying GUI toolkit is free of bugs.
The standalone DTLS test/example dependency graph also reports atty maintenance/unsoundness
warnings through its old example CLI dependencies; atty is absent from the application lockfile.

## Memory and local data

X25519 private/shared-secret objects and app-owned seed/KDF output buffers use zeroization.
AEAD keys are dropped with their owning contexts. HKDF internal state, compiler-generated
copies, allocator leftovers, JavaScript/WebView strings, clipboard, swap and crash dumps
are outside the erasure guarantee. Disconnect clears application state, not all physical
copies of data. Saved TURN credentials remain in localStorage by current local-storage policy.

Diagnostics accept safe codes and states rather than arbitrary network errors. Never add
SDP, ICE credentials/addresses, invitation URLs/tokens, SAS, encryption keys or message
contents to diagnostics, screenshots used as fixtures, CI logs or bug reports.

## Acceptance limits

Regression suites include real localhost peers and native WebView checks. They do not prove
TURN operation on a deployed server, arbitrary NAT traversal, Windows/macOS behavior, or
formal cryptographic correctness. The committed exchange is a custom protocol, not an
implementation or certification of ZRTP. Its commitment ordering follows the rationale in
[RFC 6189 section 4.4.1.1](https://www.rfc-editor.org/rfc/rfc6189.html#section-4.4.1.1).

## Personal activation

Generic releases contain no shared TURN password or bootstrap profile. A personal installer
contains a seven-day one-use activation code; its first successful claimant gets access.
The installation generates its key locally before redeeming and retries idempotently.
Windows stores the key with current-user DPAPI; Linux uses a0700 directory/0600file,
without claiming disk encryption or hardware binding. Copying that private key clones its
cryptographic identity; no hardware fingerprint is collected to detect such a copy.

Managed TURN responses accept only the configured project host/ports/transports. Custom
TURN remains explicit and unchanged. Managed credentials stay in Rust, never in localStorage
or renderer IPC. Access registry and revocation journal persist separately from RAM-only
chat invitations. Revocation stops new issuance immediately; complete relay revocation is
bounded by remaining600-second credentials plus15seconds of guard enforcement, tested
separately from invitation expiry and the lifetime of an allowed active chat.
