# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.2.0   | Current   |

## Reporting a Vulnerability

If you discover a security vulnerability in SENTINEL, please report it responsibly:

1. **Do NOT open a public GitHub issue** for security vulnerabilities
2. **Contact us on Discord**: https://discord.gg/k967Q5q6xZ (use the #security channel)
3. **Or email**: Reach out via Discord DM to the project maintainer

We will acknowledge receipt within 48 hours and provide a detailed response within 7 days.

## Security Model

### Agent Isolation via Docker

Each SENTINEL agent runs inside an **isolated Docker container** with its own:
- Linux filesystem and processes
- Chromium browser (for web tasks)
- Virtual display (Xvfb) — no access to your real screen
- Separate `/downloads` folder — downloads stay inside the container

#### Scope Isolation

When you launch an agent, you can optionally select a target directory. **Only that directory** is mounted into the container. The agent cannot see or access:

- Your home directory, SSH keys, or credentials
- System files (`C:\Windows`, `/etc`)
- Other projects or personal files
- Browser data, saved passwords, or other applications

If no directory is selected, the agent runs with an empty workspace — useful for web-only tasks.

#### Process Isolation

The agent runs as a Linux process inside Docker. It **cannot**:

- Access or control your Windows/macOS processes
- Read your clipboard or real screen content
- Open applications on your host
- Access USB devices or peripherals
- Download files to your system (downloads stay in `/downloads` inside the container)

#### Browser Isolation

The agent's Chromium browser runs inside the container's virtual display:

- **Your real browser is not affected** — the agent has its own separate browser
- The agent's browser sessions, cookies, and data are destroyed when the container stops
- Web browsing is visible to you via noVNC live view, but stays inside the container
- The agent cannot access your host network services (except via `host.docker.internal`)

#### Resource Limits

Docker enforces hard resource limits on every agent container:

- **Memory**: Configurable from Settings (default: 512 MB, max: 4096 MB)
- **Network Timeout**: Configurable per-agent
- If the agent exceeds memory limits, Docker kills the container immediately

#### Autonomy Levels

| Level | Read Files | Write Files | Write Report | Browser |
|-------|-----------|-------------|-------------|---------|
| Full Autonomy | ✅ | ✅ | ✅ | ✅ |
| Read & Report | ✅ | ❌ | ✅ | ✅ |
| Ask Before Writing | ✅ | 🔒 Ask | ✅ | ✅ |
| Read Only | ✅ | ❌ | ❌ | ✅ |

#### Disposability

Every agent container is ephemeral:

- Container is destroyed when the agent finishes or on error
- No residual processes remain on your system
- No system-level state is modified — the container is fully self-contained
- Browser history, cookies, and downloads are wiped with the container

### Data Persistence

- **Chat history and settings** are stored locally via `localStorage` in the Tauri WebView
- **API keys** are stored in `localStorage` per-provider — they persist across sessions
- **Reports** are written to your project folder (if a workspace is mounted)
- No data is sent to any server other than your chosen LLM provider

### API Key Handling

- API keys are passed to containers via **environment variables** at runtime
- Keys are stored in `localStorage` for convenience — clear your browser data to remove them
- Keys are passed only to the Docker container and your chosen LLM provider
- For maximum security, use **Ollama** (local) to keep everything on-premise

### What We Don't Protect Against

- **File damage within the mounted directory**: Unless Read-Only Mode is enabled, the agent can modify files. **Always use Git** to ensure you can revert changes.
- **Docker escape vulnerabilities**: SENTINEL relies on Docker's isolation. Keep Docker Desktop updated.
- **Data exfiltration via LLM**: The agent sends workspace contents to your LLM. Use **Ollama** for sensitive codebases.
- **Web browsing risks**: The agent's browser can visit any website. It operates in an isolated container, but network traffic exits through your connection.
- **CAPTCHA and interactive sites**: The agent uses visual browser interaction but may not always succeed with complex CAPTCHAs.

## Best Practices

1. **Use autonomy levels** — start with "Read & Report" for unfamiliar tasks
2. **Use Git** — any unwanted changes can be reverted with `git checkout .`
3. **Don't mount sensitive directories** (e.g., `~/.ssh`, `/etc`, your home folder)
4. **Use local LLMs** (Ollama) for proprietary code
5. **Keep Docker Desktop updated** for the latest security patches
6. **Review agent actions** in the live view for web-based tasks
7. **Set memory limits** in Settings to prevent resource exhaustion
