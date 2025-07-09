# ZeroID: Absolutely Secret Chat

![Rust](https://img.shields.io/badge/-Rust-000000?style=flat-square&logo=rust)

## [🇷🇺](./README.ru.md)

**ZeroID** is an absolutely secret chat for secure communication without registration, authorization, or tracking. Your device is the server, so no one can access your correspondence or message history.

## Table of Contents 📚

- [Key Features](#key-features-)
- [Supported Platforms](#supported-platforms-)
- [Screenshots](#screenshots-)
- [Tech Stack](#tech-stack-️) 
  - [Frontend](#frontend-)
  - [Backend (Core)](#backend-core-️)
- [Key Security Aspects](#key-security-aspects-)
  - [What Has Already Been Done for Security](#what-has-already-been-done-for-security-and-why)
  - [What Still Needs to Be Done for Security](#what-still-needs-to-be-done-for-security)
- [Project Structure](#project-structure-)
- [Build and Run](#build-and-run-)
- [Security](#security-)
- [Future Plans](#future-plans-)
- [Contributing](#contributing-)

## Key Features 🚀

- 🔒 **P2P Encryption** based on WebRTC and X25519, ChaCha20-Poly1305, HKDF cryptography.
- 🚫 **No Servers**: the application is completely decentralized and works without external backends.
- 🕵️‍♂️ **Complete Anonymity**: no registration, no authorization, no message history on third-party servers.
- ⚡ **High Speed and Reliability**: peer-to-peer connection with minimal latency.

## Supported Platforms 💻

- 🖥️ Windows
- 🍎 macOS
- 🐧 Linux

Android and iOS support is planned for the future.

## Screenshots 📸

| Welcome Screen | Signature Exchange (QR Codes) | Main Chat |
| -------------- | ----------------------------- | --------- |
|                |                               |           |

*(GIF demonstration will be added later)*

## Tech Stack 🛠️

### Frontend 🌐

- [React](https://react.dev/) (TypeScript)
- [Radix UI](https://www.radix-ui.com/) - UI components and hooks
- [Tailwind CSS](https://tailwindcss.com/) - styling
- [Vite](https://vitejs.dev/) - build tool
- [React Router](https://reactrouter.com/) - routing
- [React Hook Form](https://react-hook-form.com/) - form management
- [Zod](https://zod.dev/) - schema validation
- [TanStack Query](https://tanstack.com/query) - state management
- FSD architecture

### Backend (Core) ⚙️

- [Rust](https://www.rust-lang.org/) - systems programming language
- [WebRTC](https://webrtc.org/) - peer-to-peer connections
- [Ring](https://github.com/briansmith/ring) - cryptographic primitives (X25519 ECDH)
- [ChaCha20-Poly1305](https://github.com/RustCrypto/AEADs) - authenticated encryption
- [HKDF](https://github.com/RustCrypto/KDFs) - key derivation
- [Tauri](https://tauri.app/) - desktop application framework
- [Tokio](https://tokio.rs/) - async runtime
- [Serde](https://serde.rs/) - serialization/deserialization

## Key Security Aspects 🔒

### What Has Already Been Done for Security (and Why)

1. **End-to-End Encryption – ChaCha20-Poly1305 (AEAD)**
   
   *How it's implemented:* Each message is encrypted using the ChaCha20-Poly1305 algorithm; two independent `ChaCha20Poly1305` objects are created for encryption and decryption (sealing/opening).
   
   *Benefits:* Ensures confidentiality, integrity, and authentication of messages at the same time; it is impossible to break the traffic without knowing the key, and modified packets are automatically discarded.

2. **Diffie-Hellman X25519 + HKDF for Direction Separation**
   
   *How it's implemented:* The parties exchange 32-byte X25519 public keys, derive a shared secret, and pass it through HKDF-Sha256. From the 64-byte output, two independent keys are extracted: "mine → yours" and "yours → mine".
   
   *Benefits:* Even if one direction is compromised (e.g., due to sender key leakage), traffic in the opposite direction remains protected; X25519's strength is confirmed by cryptanalysis.

3. **SAS Fingerprint (Short Authentication String)**
   
   *How it's implemented:* The first 48 bits of one of the derived keys are hashed with SHA-256 and displayed as a 12-character hex code. Users can visually compare the strings.
   
   *Benefits:* Simple manual verification eliminates "man-in-the-middle" after the channel is established; if the SAS differs, the session can be immediately terminated.

4. **Zero Key "Memory" (Zeroization)**
   
   *How it's implemented:* Private keys are wrapped in a `ZeroizedKey` structure with `ZeroizeOnDrop` derivation; temporary buffers (`shared`, `okm`, k1/k2) are zeroed immediately after use; counters and SAS are wiped in the `Drop` of `CryptoCtx`.
   
   *Benefits:* In case of memory dump, swap, or crash, the chances of recovering private keys are minimized, which is especially important on desktops and mobile devices.

5. **One-Time Nonce for Each Message**
   
   *How it's implemented:* A 64-bit increment is added to the `send_n`/`recv_n` counter; it is embedded into the 96-bit ChaCha20-Poly1305 nonce.
   
   *Benefits:* No repeated nonces prevents the catastrophic "key + nonce reuse" scenario, which would break AEAD security.

6. **Basic Replay Protection**
   
   *How it's implemented:* `last_accepted_recv` is stored; messages with the same or lower number are ignored.
   
   *Benefits:* An attacker cannot endlessly replay the last packet for DoS or social engineering ("I didn't get your message").

7. **Limit on Unpacking Incoming SDP Blocks**
   
   *How it's implemented:* During GZIP decompression, `take(256 KiB)` is used, which limits the amount of data that can be extracted from compressed input.
   
   *Benefits:* Neutralizes zip-bomb attacks, where a 10 KB archive expands to gigabytes and crashes the process.

8. **Automatic State Cleanup on Disconnect**
   
   *How it's implemented:* In `emit_disconnected` and `disconnect`, all global `Mutex<Option<…>>` with keys, crypto context, and channel state are zeroed.
   
   *Benefits:* Prevents "live" keys from remaining in memory after closing the window or putting the device to sleep.

9. **DH Key Transmission Only After Channel Is Open**
   
   *How it's implemented:* The public X25519 key is sent as the first payload message over an already established DataChannel; before that, no user data is sent.
   
   *Benefits:* Reduces the window for attacks on signaling scripts and does not reveal keys prematurely.

10. **Minimization of Third-Party Dependencies**
    
    *How it's implemented:* Only trusted libraries are used: `ring`, `chacha20poly1305` (RustCrypto), and `webRTC-rs`; all sensitive code is localized in a single module.
    
    *Benefits:* Fewer potential attack surfaces and easier auditing.

### What Still Needs to Be Done for Security

**Category: Nonce Management and Overflow**  
**Problem Description:** `send_n` and `recv_n` keep growing monotonically up to `u64::MAX`, and after session restart, start again from 1. Overflow or restarting a long chat will create identical nonces.  
**Risk:** Nonce reuse in ChaCha20-Poly1305 makes message disclosure and tampering possible.

---

**Category: Replay Protection**  
**Problem Description:** Only `last_accepted_recv` is stored. If an attacker sends a message with seq +2, then repeats +1, the second packet will be accepted.  
**Risk:** Partial replay, message reordering, possible DoS scenarios.

---

**Category: Signaling Channel Integrity and DH Authentication**  
**Problem Description:** SDP blocks (OFFER/ANSWER) are still sent unsigned, and the X25519 public key is sent as the first message over an unauthenticated channel.  
**Risk:** MITM can substitute ICE parameters or perform a classic "man-in-the-middle" before users compare SAS.

---

**Category: Identifier Management**  
**Problem Description:** `random_id()` remains 64-bit. This is "enough", but a collision will occur at ≈ 4 billion calls (birthday paradox).  
**Risk:** With a hypothetically huge number of sessions, ID collision is possible (which will interfere with signaling routing).

---

**Category: Blocking Mutexes in Async Code**  
**Problem Description:** `std::sync::Mutex` is used; inside critical sections, there are calls to `emit` (I/O via Tauri).  
**Risk:** In rare conditions, the UI may "freeze", and with future expansion (e.g., file logging), a deadlock may occur.


## Project Structure 📂

```
src/
├── App.css
├── App.tsx
├── components/
│   ├── FingerprintModal.tsx
│   └── ui/ (interface components)
├── hooks/
│   ├── use-mobile.tsx
│   └── use-toast.ts
├── lib/
│   └── utils.ts
├── main.tsx
└── pages/
    ├── Chat.tsx
    ├── GenerateQR.tsx
    ├── Index.tsx
    ├── NotFound.tsx
    ├── ScanQR.tsx
    └── Welcome.tsx

Core:
src/
├── lib.rs
├── main.rs
├── signaling.rs
└── webrtc_peer.rs
```

## Build and Run 🚧

To build the application, you need to install [Tauri prerequisites](https://tauri.app/start/prerequisites/).

**Build commands:**

```bash
make build             # for Linux/macOS/Windows
make build-ios         # iOS (coming soon)
make build-android     # Android (coming soon)
```

After building, the application runs locally. No additional deployment is required.

## Security 🔑

ZeroID implements the following approaches to ensure confidentiality and security:

- 🔐 **End-to-End Encryption** using ChaCha20-Poly1305.
- 🔑 **Key Exchange** based on ECDH algorithm (X25519).
- 📱 **SAS (Short Authentication String)** for secure connection verification.
- 📵 **No Message History Storage** — data exists only on users' devices.

## Future Plans 📅

- 📢 Group chats
- 📁 File transfer
- 🎙️ Voice and video messages
- 📱 Mobile platform support (Android and iOS)

## Contributing 🤝

ZeroID is open to community contributions.

- Rust code should follow Rust standards.
- Frontend uses Feature-Sliced Design (FSD) approach.
- Before submitting a PR, ensure the code is clean and well-commented.

---

ZeroID is your right to privacy and security on the web!
