# Shared protocol v3

This crate implements the desktop and browser protocol from
[`DESKTOP_PROTOCOL.md`](../../docs/DESKTOP_PROTOCOL.md): committed X25519 exchange,
HKDF-SHA256, directional ChaCha20-Poly1305 frames, SAS, READY verification,
bounded UTF-8 message chunks and GZIP/Base64 invitations. It has no Tauri,
WebRTC, ring, network or random-number-generator dependency. Native adapters
supply operating-system entropy. Browsers supply fresh `crypto.getRandomValues`
bytes; deterministic values in tests are public fixtures only.

Build the browser module from source with Rust stable and the wasm32 target:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
bash crates/ssc-protocol/build-wasm.sh
```

The optional first argument is the generated module output directory. Generated
`pkg/` and `target/` files are ignored. CI must run the same build; no compiled
WASM blob is committed.

```js
import init, { ProtocolSession } from './ssc_protocol.js';
await init();
const seed = crypto.getRandomValues(new Uint8Array(32));
const nonce = crypto.getRandomValues(new Uint8Array(32));
const session = new ProtocolSession(connectionId, isOfferCreator, seed, nonce);
seed.fill(0);
nonce.fill(0);
```

`start()` returns commitment frames. Feed every binary DataChannel frame to
`receive(frame)`, which returns `{outgoing, sas, peer_ready, message}`. Send
`outgoing` frames in order. `sas` and `message` are null until available.
The `sas()` accessor returns undefined until the exchange finishes. Call
`confirm()` only on an explicit user action after comparing the entire SAS;
it returns an encrypted READY frame. Both users' confirmation is required for
`send_text(text, messageId16)`, whose ID also comes from `crypto.getRandomValues`.
An invalid incoming protocol frame retires the owned keys. `close()` retires
state and rejects further use; call the generated `free()` when retiring the
wrapper. No event automatically confirms a SAS.

Serialize all calls, binary Blob conversions, returned sends and complete
messages on one ordered reliable DataChannel. Reject incoming frames larger
than 70 KiB before crossing the WASM boundary. Any transport send failure must
retire the session: counters are never rolled back or retried with old frames.
The browser adapter owns transport recovery/deadlines and invitation helpers;
the protocol engine owns no timers, persistence, browser history or logging.

`encode_invitation(sessionId, 'offer'|'answer', sdp, candidatesJson)` takes
browser candidates with `candidate`, `sdpMid`, `sdpMLineIndex` keys.
`decode_invitation(value)` returns `{session_id, sdp_type, sdp, ice_candidates}`;
the latter candidates use browser keys. These use the same compact v3 codec as
desktop. Exceptions contain a fixed safe error code, never external input.

Validation:

```sh
cargo test --manifest-path crates/ssc-protocol/Cargo.toml --locked
cargo clippy --manifest-path crates/ssc-protocol/Cargo.toml --locked --all-targets -- -D warnings
bash crates/ssc-protocol/build-wasm.sh
node crates/ssc-protocol/tests/wasm.mjs
```

The tests retain all pre-extraction primitive/handshake/chunk regressions,
byte-for-byte SAS/AEAD/GZIP vectors captured from the old desktop implementation,
and deterministic complete native/WASM vectors. Actual WASM execution is checked
in Node; browser and live desktop transport acceptance are separate checks.
Clearing owned Rust buffers is best effort: no guarantee is made about browser,
JavaScript or compiler-generated copies or forensic recovery.
