# Secret Chat vendored dependency

This is the crates.io `webrtc-dtls` **0.12.0** package, retained to preserve the
WebRTC 0.13 transport API and its UDP, TCP and TLS TURN support. It is a local
dependency patch, not an upstream release or a rewritten DTLS implementation.

Origin:

- Registry: https://crates.io/crates/webrtc-dtls/0.12.0
- Original `.crate` SHA-256:
  `5ccbe4d9049390ab52695c3646c1395c877e16c15fb05d3bda8eee0c7351711c`
- Upstream repository: https://github.com/webrtc-rs/webrtc
- Upstream commit from packaged `.cargo_vcs_info.json`:
  `632dccdbf8e9638b5770ad4bd556c99d9905fd83`, subdirectory `dtls`.
- Original `LICENSE-MIT`, `LICENSE-APACHE`, tests, examples and
  `Cargo.toml.orig` are preserved. Cache markers are omitted. The standalone
  `Cargo.lock` was regenerated for the patched manifest; application builds use
  the application's lockfile.

`Cargo.toml.orig` is the immutable upstream manifest for provenance, **not** the
active manifest. Cargo builds use the patched normalized `Cargo.toml`.

Local patch (all other production source is unchanged):

1. `Cargo.toml`: update `p256` and `p384` from 0.13 to 0.14; explicitly enable
   P-384's `ecdh` feature. This allows the patched `crypto-bigint` 0.7 series
   instead of the vulnerable 0.5 series. The application lockfile and audit must
   keep `crypto-bigint` at a patched version (validated with 0.7.10).
2. `src/curve/named_curve.rs`: use the new `Generate::try_generate` system RNG
   API for P-256/P-384; propagate entropy failure. Encode public keys through
   `to_sec1_bytes`, preserving the original uncompressed SEC1 wire format.
   X25519 and its RNG are unchanged.
3. `src/prf/mod.rs`: validate and decode the peer's SEC1 bytes directly through
   the new `PublicKey::from_sec1_bytes` API, then call the same ECDH operation.
   Curve validation remains mandatory. Ring-based certificate signatures,
   cipher suites, protocol state machines and network transports are unchanged.
4. `src/prf/prf_test.rs`: add P-256/P-384 shared-secret, public-encoding and
   malformed-peer-point regression checks.

Validation from the application repository root:

```sh
CARGO_TARGET_DIR=/tmp/ssc-dtls-security-target cargo test --locked --manifest-path src-tauri/vendor/webrtc-dtls/Cargo.toml --lib
```

The explicit target directory keeps build artifacts outside this vendored tree.
After testing, `cargo clean --manifest-path src-tauri/vendor/webrtc-dtls/Cargo.toml
--target-dir /tmp/ssc-dtls-security-target` removes that temporary build directory.
The vendor tree contains no `target/` artifacts.

Replacing this patch requires keeping the application's audit and real WebRTC
tests green and checking TURN UDP/TCP/TLS compatibility. Local DTLS tests do not
prove an external TURN deployment or replace an independent cryptographic audit.


One existing trailing space in `src/conn/conn_test.rs` was removed for repository
whitespace checks. The example certificates/private keys are public upstream test
fixtures, not deployment credentials. They must never be used by a deployed service.
