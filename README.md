# ZeroID: Absolutely Secret Chat

## [🇷🇺](./README.ru.md)

**ZeroID** is an absolutely secret chat for secure communication without registration, authorization, or tracking. Your device is the server, so no one can access your correspondence or message history.

## Table of Contents 📚

- [Key Features](#key-features-)
- [Supported Platforms](#supported-platforms-)
- [Screenshots](#screenshots-)
- [Tech Stack](#tech-stack-️) 
  - [Frontend](#frontend-)
  - [Backend (Core)](#backend-core-️)
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
