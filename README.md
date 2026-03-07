# 🛡️ SENTINEL v1.0.0 (Stargenesis)

**Zero-Trust Agentic Security Orchestration Engine. CLI-Powered.**

SENTINEL is an autonomous AI agent running as a powerful, lightweight Python CLI. It bridges the gap between dynamic web auditing and deep-dive **local static codebase analysis**. By combining advanced Python-native security logic with LLM reasoning, Sentinel delivers comprehensive, automated security audits directly from your terminal.

[![Join Discord](https://img.shields.io/badge/Discord-Join%20Community-5865F2?logo=discord&logoColor=white)](https://discord.gg/k967Q5q6xZ)

---

## ⚠️ Disclaimer

> [!WARNING]
> **Legal Notice & Non-Responsibility**
> Sentinel is designed strictly for authorized security auditing, defensive analysis, and educational purposes. The creators and contributors of Sentinel are **NOT** responsible for any damages, legal issues, or consequences caused by the misuse of this tool. You may only use Sentinel on networks, systems, or codebases that you own or have explicit, documented permission to audit. Any illegal or unauthorized use of this tool is strictly prohibited.

---

## 🚀 Key Features

- **🌐 Web Audit Engine (`-audit`)**: Automatically orchestrates dynamic discovery, payload inspection, and vulnerability scanning against target URLs natively using Python's requests engine.
- **📂 Static Code Sandbox (`-analyze`)**: Analyzes local directories natively for security vulnerabilities and design flaws. *This strictly sandboxes the AI agent to the local folder you provide, ensuring memory limits and path safety.*
- **🧠 Agnostic AI Protocol**: Natively integrates with Ollama, Google Gemini, Anthropic Claude, OpenAI, Grok (xAI), and Groq.
- **🎛️ Interactive Multi-Model Mapping**: Dynamically probe local Ollama instances and easily select custom cloud models for active providers via an intuitive CLI flow.
- **📝 Automated Reporting**: Synthesizes complex scan data into professional, actionable Markdown reports (`report.md`).
- **🔐 Absolute Zero-Trust Security**: Validates all Python dependencies via live SHA-256 bootstrapping, encrypts stored API keys at rest using HKDF AES, and relies purely on atomic file IO operations.

---

## 🏗️ Architecture

```
┌─────────────────────┐
│   Python CLI App    │ ◄─ interactive loop (-audit, -analyze, -config)
└─────────┬───────────┘
          │ Subprocess Engine
          ├─────────────────────────────────┐
          │ (Dynamic Web Scans)             │ (Local Folder Scans)
  ┌───────▼───────┐                 ┌───────▼───────┐
  │ HTTP Engine   │                 │ Native Python │
  │  ┌─────────┐  │                 │  ┌─────────┐  │
  │  │ Session │  │ ← Requests &    │  │ Path    │  │ ← Strictly bounded to targeted folder
  │  │ Sandbox │  │   Live Audits   │  │ Sandbox │  │
  │  └─────────┘  │                 │  └─────────┘  │
  └───────┬───────┘                 └───────┬───────┘
          │                                 │ 
          │ Extracted Context               │ 
  ┌───────▼─────────────────────────────────▼───────┐
  │                 LLM Processor                   │ ← Analyzes outputs via Cloud APIs or Local Ollama
  └─────────────────────────────────────────────────┘
```

---

## 🛠️ Getting Started

### Prerequisites

- **[Python 3.9+](https://www.python.org/downloads/)**
- **[Ollama](https://ollama.com/)** (Optional but Recommended) — For running models completely locally, recommended for `-analyze` privacy.

### Installation

Clone the repository and install the strict dependencies via `pip`:

```bash
git clone https://github.com/Doom-pixel/sentinel.git
cd sentinel
pip install -r requirements.txt
pip install -e .
```

*Note: As a zero-trust platform, Sentinel explicitly checks the exact SHA-256 hashes of the files installed via `requirements.txt` at runtime. Modifications to these core libraries will halt the engine.*

### Usage

Because the package implements `entry_points`, installing it via Pip automatically adds Sentinel to your system PATH. You can launch Sentinel from anywhere simply by typing:

```bash
sentinel
```

*Alternatively, you can run the script directly from the project folder via `python sentinel.py`.*

**Available Commands:**
- `-analyze [path]`: Run the static code sandbox against a local path natively. If no path is provided, it prompts to analyze the current directory.
- `-audit <url>`: Run the native Python web audit engine against a target (e.g., `-audit https://example.com/`).
- `-recon <domain>`: Run a stealthy passive reconnaissance intelligence gather natively (e.g., `-recon example.com`).
- `-vuln <url>`: Run aggressive targeted vulnerability scanning natively (e.g., `-vuln https://example.com/`).
- `-report <file.md>`: Compile any generated Markdown report into a locally styled HTML web page.
- `-history`: View an executable history log of all your previous analysis runs.
- `-config`: Run the setup wizard to configure API keys.
- `-vp`: View currently configured providers.
- `-ep`: Enable additional providers without deleting existing ones.
- `-llm`: Configure custom target models for active providers.
- `clear`: Clear the terminal screen.

---

## 🔑 LLM Providers

| Provider | Requirement | Privacy Warning |
|----------|-------------|-----------------|
| **Ollama** | Local installation running on `localhost:11434`. | **Safe** - Code never leaves your machine. |
| **Google Gemini** | Valid API Key. | Data sent to cloud during `-analyze`. |
| **Anthropic Claude** | Valid API Key. | Data sent to cloud during `-analyze`. |
| **OpenAI** | Valid API Key. | Data sent to cloud during `-analyze`. |
| **Grok (xAI)** | Valid API Key. | Data sent to cloud during `-analyze`. |
| **Groq** | Valid API Key. | Data sent to cloud during `-analyze`. |

*You can configure specific models for each of these providers using the `-llm` command in the CLI. Sentinel will explicitly warn you before sending local code to cloud providers during the `-analyze` flow.*

---

## ⚖️ License

This project is licensed under the **MIT License**.
See the [LICENSE](https://github.com/Doom-pixel/sentinel/blob/main/LICENSE) file for details.

## 💬 Community

**Discord:** https://discord.gg/k967Q5q6xZ
