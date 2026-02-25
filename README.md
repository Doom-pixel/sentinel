# 🔒 SENTINEL

**Autonomous. Capability-Gated. Auditable.**

SENTINEL is a high-security agentic framework designed for autonomous operations with a focus on trust, safety, and transparency. It provides a robust infrastructure for AI agents to operate within strict, verifiable boundaries, ensuring that every action is authorized, audited, and safe.

---

## 🚀 Key Features

- **🛡️ Capability-Gated Security**: Fine-grained control over system resources (filesystem, network, UI) using cryptographic tokens and scope validation.
- **🤝 Human-In-The-Loop (HITL)**: Seamless approval workflows for critical or high-risk actions, ensuring human oversight where it matters most.
- **📢 Multi-Channel Notifications**: Real-time alerts via Discord, Slack, and Telegram using HTTP webhooks for immediate situational awareness.
- **📝 Automated Security Auditing**: Built-in agents that autonomously audit the codebase and runtime operations, generating detailed security reports.
- **💻 Cross-Platform UI**: A modern, premium Tauri-powered desktop interface for monitoring and managing agent activities.

---

## 🏗️ Architecture

SENTINEL is built as a modular system in Rust, leveraging WebAssembly (Wasm) for safe guest execution.

```mermaid
graph TD
    A[SENTINEL Host] -->|Capability Gating| B[Capability Manager]
    A -->|HITL Bridge| C[Tauri UI]
    A -->|Execution| D[SENTINEL Guest (Wasm)]
    D -->|Host Calls| A
    B -->|Validation| E[Filesystem/Network]
    A -->|Alerts| F[Notifications Service]
    F -->|Webhook| G[Discord/Slack/Telegram]
```

### Components:
- **`sentinel-host`**: The core engine that manages execution, capabilities, and HITL routing.
- **`sentinel-guest`**: High-level agent logic designed to run within the host's sandbox.
- **`sentinel-guest-api`**: The interface definition for guest-host communication.
- **`sentinel-shared`**: Common types and utilities used across the project.
- **`sentinel-ui`**: The Tauri + React frontend application.

---

## 🛠️ Getting Started

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Node.js & npm](https://nodejs.org/) (for the UI)
- [Tauri Dependencies](https://tauri.app/v1/guides/getting-started/prerequisites)

### Backend (Host)
```bash
cd sentinel-host
cargo build --release
```

### Frontend (UI)
```bash
cd sentinel-ui
npm install
npm run tauri dev
```

---

## ⚖️ License

Project is under the MIT License. See `LICENSE` for more details.
