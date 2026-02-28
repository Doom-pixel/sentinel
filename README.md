# 🛡️ SENTINEL

**Your Personal AI Agent. Docker-Isolated. Fully Autonomous.**

SENTINEL is a personal AI agent that runs inside **Docker containers** with full OS-level isolation. Ask it anything — browse the web, analyze code, send emails, run shell commands, research topics — all from a beautiful desktop app. The agent has its own browser, file system, and tools, completely isolated from your system. Currently, Sentinel is better in cybersecurity stuff like analyzing a codebase for security vulnerabilities for example. A lot of bugs can show up, please, report them as soon as possibile so we can fix them as fast as we can.

[![Join Discord](https://img.shields.io/badge/Discord-Join%20Community-5865F2?logo=discord&logoColor=white)](https://discord.gg/k967Q5q6xZ)

<img width="960" height="564" alt="image" src="https://github.com/user-attachments/assets/2390f92b-1e83-4262-9ce9-d5105bdb59f8" />

---

## 🚀 Key Features

- **🐳 Docker Isolation**: Each agent runs in its own container — full OS-level sandboxing, zero risk to your system
- **🌐 Web Browsing**: Agent has Chromium built-in, can browse websites, fill forms, search Google — with live view in your chat
- **🖥️ Live View**: Watch the agent's screen in real-time via noVNC, with replay timebar for reviewing past actions
- **🧠 Any LLM**: Ollama (local), OpenAI GPT-5.2, Anthropic Claude sonnet 4.6, Anthropic Claude opus 4.6, Google Gemini 3.1, Google Gemini 3 flash, Deepseek, Grok 4.20 — or type any custom model
- **💬 Chat Interface**: Full conversation with the agent — send follow-up messages, get markdown-formatted responses
- **🎛️ Autonomy Levels**: Full, Read & Report, Ask Before Writing, or Read Only
- **📝 Auto Reports**: Summary in chat + detailed report saved to your project folder
- **📢 Notifications**: Discord, Slack, and Telegram webhooks
- **🔄 Auto-Update**: Notified when a new version is available
- **💾 Persistent State**: Chat history, settings, and preferences saved across sessions
- **📁 Optional Workspace**: Agent can work with or without a project folder

---

## 🏗️ Architecture

```
┌──────────────────────────────┐
│   Tauri Desktop App          │
│ ┌──────────┐ ┌─────────────┐ │
│ │ React UI │ │ Rust Backend │◄── manages containers via bollard
│ └──────────┘ └──────┬──────┘ │
└─────────────────────┼────────┘
                      │ Docker API
              ┌───────▼───────┐
              │ Docker Engine │
              │  ┌──────────┐ │
              │  │ Agent VM │ │  ← Chromium + noVNC + ffmpeg
              │  │          │ │  ← sentinel-agent (tool-use loop)
              │  │          │ │  ← /workspace + /downloads
              │  └──────────┘ │
              └───────────────┘
```

### Agent Container Stack

| Component | Purpose |
|-----------|---------|
| **sentinel-agent** | Rust binary with tool-use loop (up to 15 iterations) |
| **Chromium** | Full browser for web tasks |
| **noVNC** | Streams agent's screen to your desktop app |
| **ffmpeg** | Records screen for replay |
| **openbox** | Lightweight window manager |
| **/workspace** | Mounted project folder (optional) |
| **/downloads** | Isolated download folder (not mounted to host) |

### Agent Tools

The agent has 6 built-in tools it can use autonomously:

| Tool | Description |
|------|-------------|
| `read_file` | Read any file in the workspace |
| `write_file` | Write/create files |
| `list_files` | Browse directory contents |
| `shell` | Run shell commands inside the container |
| `browse` | Open URLs in Chromium (visible in live view) |
| `search_web` | Search Google and view results |

---

## 🐳 How Isolation Works

```
Your PC                              Docker Container
──────                               ────────────────
C:\Users\you\my-app\  ◄═ bind mount ═►  /workspace/
                                         /downloads/  ← isolated, not on host
Everything else:       ❌ INVISIBLE      Agent has its own:
  ~/.ssh/              ❌ Not mounted      • Browser
  C:\Windows\          ❌ Not mounted      • File system
  Other projects/      ❌ Not mounted      • Network (for LLM + web)
```

| Layer | Protection |
|-------|-----------|
| **Scope Isolation** | Agent only sees the directory you select (or nothing) |
| **Process Isolation** | Runs inside Linux, can't access your Windows processes |
| **Download Isolation** | Downloads stay in `/downloads` inside the container |
| **Memory Limits** | Docker enforces caps (configurable, default 512 MB) |
| **Autonomy Levels** | From full access to read-only mode |
| **Disposability** | Destroy the container instantly — zero cleanup |

---

## 🛠️ Getting Started

### Prerequisites

- **[Docker Desktop](https://www.docker.com/products/docker-desktop/)** — required for agent containers
- **[Rust](https://www.rust-lang.org/tools/install)** — latest stable (for building from source)
- **[Node.js & npm](https://nodejs.org/)** — for the UI

### Quick Start

```bash
# 1. Build the agent Docker image
docker build -t sentinel-agent:latest -f docker/Dockerfile .

# 2. Install UI dependencies
cd sentinel-ui && npm install

# 3. Launch in dev mode
npx tauri dev
```

### Build Installer (.exe)

```bash
cd sentinel-ui
npx tauri build
# Output: src-tauri/target/release/bundle/nsis/Sentinel_0.2.0_x64-setup.exe
```

---

## 🔑 LLM Providers

| Provider | Models | Local? | API Key? |
|----------|--------|--------|----------|
| **Ollama** | llama3.3, qwen2.5, mistral, deepseek-r1 | ✅ | No |
| **OpenAI** | gpt-4.1, gpt-4.1-mini, gpt-4.1-nano, o3-mini | ❌ | Yes |
| **Anthropic** | claude-sonnet-4, claude-3.5-haiku, claude-3.5-sonnet | ❌ | Yes |
| **Deepseek** | deepseek-chat, deepseek-reasoner | ❌ | Yes |
| **xAI** | grok-3, grok-3-mini | ❌ | Yes |
| **Google** | gemini-2.5-flash, gemini-2.5-pro | ❌ | Yes |

> **Custom models**: Click "Custom ↗" in the model selector to type any model name.

For **Ollama**, install from [ollama.com](https://ollama.com) and pull a model: `ollama pull [model's name]`

---

## ⚖️ License

This project is licensed under the **MIT License**.
See the `LICENSE`file for more precision.

## 💬 Community

**Discord:** https://discord.gg/k967Q5q6xZ
