# Local TURN 0.10.0 patch

The sources are the published `turn` 0.10.0 crate from webrtc-rs, retained under
the upstream dual MIT / Apache-2.0 licenses (`LICENSE-MIT`, `LICENSE-APACHE`).

`ClientInternal::perform_transaction` now propagates `TransactionResult.err`
instead of returning a successful transaction containing a default STUN message.
The unmodified implementation lets a timed-out CreatePermission be cached as
Permitted, so `RelayConn::send_to` reports success even though the TURN server
never replied. No transport, credential, permission or cryptographic policy is
relaxed. No dependency vulnerability exception is introduced.

Two focused tests in `src/client/client_test.rs` document the evidence:

- `ssc_permission_timeout_must_not_be_reported_as_a_successful_send` uses a
  bound-but-silent loopback TURN socket. It fails on the original branch:
  `send_to` returns success after retransmissions expire. It passes with error
  propagation.
- `ssc_data_before_permission_response_does_not_deadlock_reader` forces a DATA
  indication to be consumed while the first CreatePermission response is withheld;
  the receiver then sends through the same allocation to a different peer IP.
  It passes both before and after the patch. The receiver uses a separate channel,
  so this is a negative control against the proposed allocation/reader deadlock.

Both tests use synthetic credentials, ephemeral loopback ports and controlled
STUN responses. They do not contact public STUN servers. They demonstrate the
specific timeout defect, not the cause of every observed connectivity failure.

```sh
cargo test --manifest-path src-tauri/vendor/turn/Cargo.toml --locked --lib ssc_
cargo test --manifest-path src-tauri/vendor/turn/Cargo.toml --locked --lib test_client_nonce_expiration
cargo audit --file src-tauri/vendor/turn/Cargo.lock
```

The upstream crate also retains tests that contact external Google STUN servers;
the focused regression gate above does not depend on public network availability.
The standalone ICE crate and desktop application both patch the same local TURN
source, so production and transport regression tests use this implementation.

The receive loop also discards a per-datagram `handle_inbound` error and continues
reading. Previously, a foreign UDP STUN Binding request stopped the shared TURN
control reader, so an already queued legitimate permission response was lost.
I/O errors and explicit cancellation still stop the reader; invalid datagrams
are neither accepted as application data nor forwarded. The diagnostic message
is fixed text and does not format packet contents or the external error.

`ssc_foreign_binding_request_must_not_kill_turn_reader` is red on the published
read-loop branch and green with this change. The otherwise identical raw-DTLS
control passes both versions, since that packet was already ignored rather
than treated as a fatal request. This defect does not itself delete a TURN
allocation; allocation-mismatch responses still require separate investigation.
