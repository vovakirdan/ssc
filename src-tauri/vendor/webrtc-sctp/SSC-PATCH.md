# SSC SCTP packet budget patch

## Provenance

The starting point is the complete published `webrtc-sctp` **0.12.0** crate
(71 files), extracted from the registry archive without modifying its manifests
or licenses:

- Archive: <https://crates.io/api/v1/crates/webrtc-sctp/0.12.0/download>
- Archive SHA256: `07439c134425d51d2f10907aaf2f815fdfb587dce19fe94a4ae8b5faf2aae5ae`
- Upstream commit from `.cargo_vcs_info.json`:
  `fa3b850527f5db19f8e7d844e8356e1d3676f616`, directory `sctp`.
- Source: <https://github.com/webrtc-rs/webrtc/tree/fa3b850527f5db19f8e7d844e8356e1d3676f616/sctp>
- Original `LICENSE-MIT` and `LICENSE-APACHE` are retained.

The published archive omits two directories required by its existing fuzz
regression tests. Seven **real, unchanged** crash fixtures (726 bytes total)
were retrieved from `sctp/fuzz/artifacts/{packet,param}` at that exact commit.
`FUZZ-CORPUS.json` records every upstream Git blob SHA1 and local SHA256. All
Git blob hashes were independently verified; no empty corpus or skip was added.
The same upstream project licenses apply to these fixtures.

## Narrow transport changes

1. Set the conservative initial outgoing SCTP packet size from 1228 to **1100
   bytes**. The existing stream fragmenter consequently uses at most **1072
   DATA payload bytes** (12-byte packet header plus 16-byte DATA header).
2. Account for the 16-byte DATA header and four-byte alignment padding when
   bundling ordinary DATA chunks. Fast retransmission uses the same serialized
   size calculation, including padding.
3. Add three regression tests using actual `Packet::marshal()` output, rather
   than only the size estimator. The original implementation produces a
   1108-byte packet for payloads `[1000,64]` and a 1104-byte fast retransmission
   for `[1,1055]` under a 1100-byte budget. Both tests were observed failing
   before the patch. Full 1072-byte payloads serialize to exactly 1100 bytes;
   mixed short/full chunks also stay within the budget without losing chunks.

The current outer budget is at most 65 bytes for a CBC DTLS record, 59 for a
TURN indication with IPv6 peer address/fingerprint/padding, and 48 for IPv6/UDP:
`1100 + 65 + 59 + 48 = 1272 <= 1280`. The target path was independently measured
as 1280 bytes; the interface MTU alone did not describe that path.

The strict serialized 1100-byte bound here covers normal DATA bundles and fast
DATA retransmission packets. Control/handshake packet builders are unchanged;
this patch does not impose a universal cap on every SCTP packet type.

[RFC 8261 section 5](https://datatracker.ietf.org/doc/html/rfc8261#section-5)
recommends a conservative SCTP fallback when DF control is unavailable.
[RFC 8200 section 5](https://www.rfc-editor.org/rfc/rfc8200.html#section-5)
defines the 1280-byte IPv6 minimum. This patch is **not** PLPMTUD and does not
promise support for every smaller network MTU or future encapsulation. A
configurable upstream transmit budget/full PLPMTUD remains upstream debt.

The maximum DATA payload decreases from 1200 to 1072 bytes (10.7%), so packet
count and overhead can increase. No throughput benchmark is claimed. Ciphers,
authentication, replay checks, SCTP reliability, message limits, server quotas,
OS/VPN settings and the application wire protocol are unchanged.

## Locks and audit boundary

The application `Cargo.toml` adds a path patch. Its transport dependency lock
change is only SCTP registry source/checksum to path resolution, apart from
normal application-version metadata; **all application registry versions and
dependency edges are unchanged**.

The standalone vendor lock intentionally differs from the published lock:
`bytes 1.10.1 -> 1.12.1`, `rand 0.8.5 -> 0.8.8`, and
`tokio 1.45.0 -> 1.54.1`, matching the already audited application versions.
Necessary resolver changes are `bitflags 2.9.0 -> 2.13.2`,
`libc 0.2.172 -> 0.2.191`, `mio 1.0.3 -> 1.2.4`,
`redox_syscall 0.5.12 -> 0.5.18`, `socket2 0.5.9 -> 0.6.5`,
`tokio-macros 2.5.0 -> 2.7.2`, `wasi 0.11.0 -> 0.11.1`, and
`windows-sys 0.52.0 -> 0.61.2`; `syn 3.0.10` and `windows-link 0.2.1` are added.
Unused old backtrace packages (`addr2line`, `adler2`, `backtrace`, `gimli`,
`miniz_oxide`, `object`, `rustc-demangle`) disappear. No runtime manifest
dependency or feature was added. Upstream `.gitignore` ignores `Cargo.lock`;
this specific standalone lock must be force-added when committing.

Fresh RustSec audit on 2026-11-10 reports zero vulnerabilities, with these
explicit informational warnings (no advisory ignore list):

- `atty 0.2.14`: `RUSTSEC-2024-0375` (unmaintained) and
  `RUSTSEC-2021-0145` (unsound). `cargo tree -i atty` confirms the path is only
  the standalone **dev dependency** `clap 3.2.25`.
- `pin-utils 0.1.0`: `RUSTSEC-2026-0441` (unmaintained). This is **not dev-only**:
  the path is `webrtc-sctp -> webrtc-util -> nix -> pin-utils`.

## Verification and known upstream gates

```sh
cargo test --manifest-path src-tauri/vendor/webrtc-sctp/Cargo.toml --locked --lib
cargo test --manifest-path src-tauri/vendor/webrtc-sctp/Cargo.toml --locked --lib ssc_
cargo fmt --manifest-path src-tauri/vendor/webrtc-sctp/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Measured with this patch: all **127 standalone library tests**, all **91 native
application tests**, native strict Clippy and formatting pass. The three
`ssc_` tests retain the old-red/new-green serialized-packet evidence.

Two unrelated published-package gates remain explicit:

- Building examples via unrestricted `cargo test` fails because existing
  `ping`/`pong` examples import `tokio::signal` but the published manifest does
  not enable that feature. The manifest/examples are preserved.
- Standalone `clippy --lib -- -D warnings` finds six upstream lint errors:
  manual saturating subtraction, `io::ErrorKind::Other`, draining/collecting,
  and three `async_trait`-generated `double_must_use` errors. No lint suppression
  or unrelated SDK cleanup was added. Application strict Clippy passes.

Independent review passed. A proper-Tauri **local 0.4.2 candidate** then passed
one public-HTTPS/native-creator relay case against the unchanged public web
client: 1 KiB and 1 MiB in both directions, full SAS verification, manual
consent, original filenames, real Blob downloads with independent SHA256,
interleaved text/reactions and authenticated exit/history clearing. OS/VPN,
server policies and rate limits were not changed for that case.

This local candidate is not a published release artifact. Acceptance of the
final published artifact, other platforms and physical microphone/capture
behavior remains separate from the library tests and this candidate case.
