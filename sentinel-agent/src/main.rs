//! # SENTINEL Agent
//!
//! General-purpose personal AI agent running inside a Docker container.
//! Supports: file operations, web browsing (via Chromium), shell commands,
//! sub-agent delegation, and communicates with Tauri host via HTTP callbacks.
//!
//! The agent uses a tool-use loop:
//! 1. Send context + available tools to LLM
//! 2. LLM returns text or a tool call
//! 3. Execute the tool, feed result back
//! 4. Repeat until LLM says "done"

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::process::Command;
use walkdir::WalkDir;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct LogPayload {
    level: String,
    target: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct AgentStatus {
    agent_id: String,
    status: String,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct CompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

// ── Callback Client ─────────────────────────────────────────────────────────

struct HostCallback {
    client: reqwest::Client,
    callback_url: String,
    agent_id: String,
}

impl HostCallback {
    fn new(callback_url: String, agent_id: String) -> Self {
        Self { client: reqwest::Client::new(), callback_url, agent_id }
    }

    async fn log(&self, level: &str, target: &str, message: &str) {
        let payload = LogPayload {
            level: level.to_string(),
            target: format!("{}::{}", self.agent_id, target),
            message: message.to_string(),
        };
        let _ = self.client.post(format!("{}/log", self.callback_url))
            .json(&payload).send().await;
        eprintln!("[{}] {} {}", level.to_uppercase(), target, message);
    }

    /// Send a thought that will display as a chat bubble in the UI.
    /// The entire message is sent as ONE log entry so multi-line content stays together.
    async fn thought(&self, msg: &str) {
        // Send as a single log entry — the frontend parses "THOUGHT:" prefix
        self.log("info", "agent", &format!("THOUGHT: {}", msg)).await;
    }

    async fn status(&self, status: &str, message: &str) {
        let payload = AgentStatus {
            agent_id: self.agent_id.clone(),
            status: status.to_string(),
            message: message.to_string(),
        };
        let _ = self.client.post(format!("{}/status", self.callback_url))
            .json(&payload).send().await;
    }

    async fn gui_active(&self, active: bool) {
        let _ = self.client.post(format!("{}/gui", self.callback_url))
            .json(&serde_json::json!({
                "agent_id": self.agent_id,
                "gui_active": active,
            })).send().await;
    }
}

// ── LLM Client ──────────────────────────────────────────────────────────────

struct LlmClient {
    client: reqwest::Client,
    provider: String,
    model: String,
    api_key: String,
    base_url: String,
}

impl LlmClient {
    fn new(provider: &str, model: &str, api_key: &str) -> Self {
        let base_url = match provider {
            "ollama" => "http://host.docker.internal:11434".to_string(),
            "openai" => "https://api.openai.com/v1".to_string(),
            "anthropic" => "https://api.anthropic.com/v1".to_string(),
            "deepseek" => "https://api.deepseek.com/v1".to_string(),
            "grok" => "https://api.x.ai/v1".to_string(),
            "google" => "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            other => other.to_string(),
        };
        Self {
            client: reqwest::Client::new(),
            provider: provider.to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            base_url,
        }
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        if self.provider == "ollama" {
            let req = OllamaRequest {
                model: self.model.clone(),
                messages: messages.to_vec(),
                stream: false,
            };
            let resp = self.client
                .post(format!("{}/api/chat", self.base_url))
                .json(&req).send().await.context("Ollama request failed")?
                .json::<OllamaResponse>().await.context("Failed to parse Ollama response")?;
            Ok(resp.message.content)
        } else {
            let req = CompletionRequest {
                model: self.model.clone(),
                messages: messages.to_vec(),
                max_tokens: Some(4096),
                temperature: Some(0.2),
            };
            let mut http_req = self.client
                .post(format!("{}/chat/completions", self.base_url))
                .json(&req);
            if !self.api_key.is_empty() {
                http_req = http_req.bearer_auth(&self.api_key);
            }
            let resp_text = http_req.send().await.context("LLM request failed")?
                .text().await.context("Failed to read LLM response")?;
            
            // Try parsing as standard response
            match serde_json::from_str::<CompletionResponse>(&resp_text) {
                Ok(parsed) => Ok(parsed.choices.first().map(|c| c.message.content.clone()).unwrap_or_default()),
                Err(_) => {
                    // Log raw response for debugging
                    eprintln!("[DEBUG] Raw LLM response: {}", &resp_text[..resp_text.len().min(500)]);
                    Err(anyhow::anyhow!("Failed to parse LLM response: {}", &resp_text[..resp_text.len().min(200)]))
                }
            }
        }
    }
}

// ── Tool Execution ──────────────────────────────────────────────────────────

fn execute_tool(tool_name: &str, args: &str, target_dir: &str) -> String {
    match tool_name {
        "read_file" => {
            let path = if args.starts_with('/') { args.to_string() } else { format!("{}/{}", target_dir, args) };
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if content.len() > 15_000 {
                        // Safe truncation at char boundary
                        let truncated: String = content.chars().take(15_000).collect();
                        format!("{}\n\n[TRUNCATED — showing first 15KB of {}KB]", truncated, content.len() / 1024)
                    } else { content }
                }
                Err(e) => format!("Error reading {}: {}", path, e),
            }
        }
        "write_file" => {
            let parts: Vec<&str> = args.splitn(2, "\n---CONTENT---\n").collect();
            if parts.len() < 2 { return "Error: write_file format must be 'path\\n---CONTENT---\\ncontent'".to_string(); }
            let file_path = parts[0].trim();
            let path = if file_path.starts_with('/') { file_path.to_string() } else { format!("{}/{}", target_dir, file_path) };
            // Create parent directories if needed
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&path, parts[1]) {
                Ok(_) => format!("Written {} bytes to {}", parts[1].len(), path),
                Err(e) => format!("Error writing {}: {}", path, e),
            }
        }
        "list_files" => {
            let dir = if args.trim().is_empty() { target_dir } else { args.trim() };
            let mut files = Vec::new();
            for entry in WalkDir::new(dir).max_depth(3).into_iter()
                .filter_entry(|e| {
                    let n = e.file_name().to_string_lossy();
                    !["target", "node_modules", ".git", "dist", "build", "__pycache__"].contains(&n.as_ref())
                })
            {
                if let Ok(e) = entry {
                    if e.file_type().is_file() {
                        files.push(e.path().to_string_lossy().replace(target_dir, "."));
                    }
                }
            }
            if files.is_empty() { "No files found.".to_string() }
            else { files.join("\n") }
        }
        "shell" => {
            let cmd = args.trim();
            match Command::new("sh").arg("-c").arg(cmd)
                .current_dir(target_dir)
                .output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let mut result = String::new();
                    if !stdout.is_empty() { result.push_str(&stdout); }
                    if !stderr.is_empty() { result.push_str(&format!("\n[stderr] {}", stderr)); }
                    if result.len() > 10_000 { result.truncate(10_000); result.push_str("\n[TRUNCATED]"); }
                    if result.is_empty() { "(no output)".to_string() } else { result }
                }
                Err(e) => format!("Shell error: {}", e),
            }
        }
        "browse" => {
            let url = args.trim();
            let screenshot_path = "/tmp/screenshot.png";
            let _ = Command::new("sh").arg("-c")
                .arg(format!(
                    "DISPLAY=:99 chromium --no-sandbox --disable-gpu --headless=new --screenshot={} --window-size=1280,720 '{}' 2>/dev/null",
                    screenshot_path, url
                )).output();
            
            if std::path::Path::new(screenshot_path).exists() {
                format!("Browser navigated to: {}\nScreenshot saved to {}\nNote: The live browser is visible in the noVNC stream.", url, screenshot_path)
            } else {
                let _ = Command::new("sh").arg("-c")
                    .arg(format!("DISPLAY=:99 chromium --no-sandbox --disable-gpu '{}' &", url))
                    .output();
                format!("Opened {} in the browser. The user can see this in the live view.", url)
            }
        }
        "search_web" => {
            let query = args.trim().replace(' ', "+");
            let url = format!("https://www.google.com/search?q={}", query);
            let _ = Command::new("sh").arg("-c")
                .arg(format!("DISPLAY=:99 chromium --no-sandbox --disable-gpu '{}' &", url))
                .output();
            format!("Searching the web for: {}\nOpened in browser. Results visible in live view.", args.trim())
        }
        "delegate" => {
            // Sub-agent: args = "task description for sub-agent"
            // Runs a mini tool-use loop inline with reduced iterations
            format!("[Sub-agent spawned for task: {}]", args.trim())
        }
        _ => format!("Unknown tool: {}", tool_name),
    }
}

fn parse_tool_call(response: &str) -> Option<(String, String)> {
    // Look for tool calls in format: [TOOL:tool_name] args [/TOOL]
    let start = response.find("[TOOL:")?;
    // Find the closing ] AFTER the [TOOL: start
    let rest = &response[start + 6..];
    let end_bracket = rest.find(']')?;
    let tool_name = rest[..end_bracket].trim().to_string();
    let after_tag = &rest[end_bracket + 1..];
    let args = if let Some(end) = after_tag.find("[/TOOL]") {
        after_tag[..end].trim().to_string()
    } else {
        after_tag.trim().to_string()
    };
    Some((tool_name, args))
}

// ── File Discovery ──────────────────────────────────────────────────────────

fn discover_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    if !std::path::Path::new(dir).exists() { return files; }
    for entry in WalkDir::new(dir).max_depth(4).into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !["target", "node_modules", ".git", "dist", "build", "__pycache__", ".next"].contains(&n.as_ref())
        })
    {
        if let Ok(e) = entry {
            if e.file_type().is_file() {
                files.push(e.path().to_string_lossy().to_string());
            }
        }
    }
    files
}

fn read_file_safe(path: &str, max_bytes: usize) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            if content.len() > max_bytes {
                let truncated: String = content.chars().take(max_bytes).collect();
                Some(format!("{}...\n\n[TRUNCATED]", truncated))
            } else { Some(content) }
        }
        Err(_) => None,
    }
}

// ── Determine if task needs GUI ─────────────────────────────────────────────

fn needs_gui(task: &str) -> bool {
    let gui_keywords = [
        "browse", "browser", "website", "web page", "navigate", "search the web",
        "google", "download from", "open url", "visit", "order", "buy",
        "send email", "read email", "gmail", "youtube", "twitter", "reddit",
    ];
    let lower = task.to_lowercase();
    gui_keywords.iter().any(|kw| lower.contains(kw))
}

// ── Sub-Agent ───────────────────────────────────────────────────────────────

async fn run_subagent(
    llm: &LlmClient,
    host: &HostCallback,
    task: &str,
    target_dir: &str,
    parent_context: &str,
) -> String {
    host.thought(&format!("🔀 Delegating sub-task: *{}*", task)).await;

    let system_prompt = format!(
        "You are a Sentinel sub-agent executing a specific sub-task. \
        You have access to the same tools as the main agent. \
        Complete the task and respond with [DONE] followed by your result.\n\n\
        Parent context: {}\n\n\
        ## Available Tools\n\
        You can call tools by writing [TOOL:tool_name] args [/TOOL].\n\
        Tools: read_file, write_file, list_files, shell, browse, search_web\n\n\
        ## Response Format\n\
        - Use ONE tool per message.\n\
        - When done, respond with [DONE] and your complete result.\n",
        parent_context
    );

    let mut messages = vec![
        ChatMessage { role: "system".into(), content: system_prompt },
        ChatMessage { role: "user".into(), content: task.to_string() },
    ];

    let max_sub_iterations = 8;
    for _ in 0..max_sub_iterations {
        let response = match llm.chat(&messages).await {
            Ok(r) => r,
            Err(e) => return format!("Sub-agent error: {}", e),
        };

        if response.contains("[DONE]") {
            let result = response.replace("[DONE]", "").trim().to_string();
            host.thought(&format!("✅ Sub-task completed: {}", &result[..result.len().min(200)])).await;
            return result;
        }

        if let Some((tool_name, tool_args)) = parse_tool_call(&response) {
            host.log("info", "sub-agent", &format!("Using tool: {}", tool_name)).await;
            let result = execute_tool(&tool_name, &tool_args, target_dir);
            messages.push(ChatMessage { role: "assistant".into(), content: response });
            messages.push(ChatMessage { role: "user".into(), content: format!("[Tool Result for {}]\n{}", tool_name, result) });
        } else {
            messages.push(ChatMessage { role: "assistant".into(), content: response });
            messages.push(ChatMessage { role: "user".into(), content: "Continue. Use tools if needed, or [DONE] with your result.".to_string() });
        }
    }

    "Sub-agent reached max iterations without completing.".to_string()
}

// ── Main Agent Logic ────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let callback_url = env::var("SENTINEL_CALLBACK_URL").unwrap_or_else(|_| "http://host.docker.internal:9876".to_string());
    let agent_id = env::var("SENTINEL_AGENT_ID").unwrap_or_else(|_| "agent-001".to_string());
    let provider = env::var("SENTINEL_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
    let model = env::var("SENTINEL_MODEL").unwrap_or_else(|_| "llama3.1:8b".to_string());
    let api_key = env::var("SENTINEL_API_KEY").unwrap_or_default();
    let target_dir = env::var("SENTINEL_TARGET_DIR").unwrap_or_else(|_| "/workspace".to_string());
    let task = env::var("SENTINEL_TASK").unwrap_or_else(|_| "Help me with this project".to_string());
    let autonomy = env::var("SENTINEL_AUTONOMY").unwrap_or_else(|_| "read_report".to_string());

    let host = HostCallback::new(callback_url, agent_id);
    let llm = LlmClient::new(&provider, &model, &api_key);

    host.log("info", "agent", "═══ SENTINEL Agent starting ═══").await;
    host.thought(&format!("Task received: **{}**", task)).await;
    host.log("info", "agent", &format!("Provider: {} ({})", provider, model)).await;
    host.status("running", "Agent started").await;

    // Determine if GUI is needed
    let use_gui = needs_gui(&task);
    if use_gui {
        host.gui_active(true).await;
        host.thought("This task requires a browser. Opening the live view...").await;
    }

    // Build workspace context
    let has_workspace = std::path::Path::new(&target_dir).exists() && 
        std::fs::read_dir(&target_dir).map(|mut d| d.next().is_some()).unwrap_or(false);

    let workspace_overview = if has_workspace {
        let files = discover_files(&target_dir);
        let file_list: Vec<String> = files.iter().map(|f| f.replace(&target_dir, ".")).collect();
        let preview: Vec<&String> = file_list.iter().take(40).collect();
        format!("Files:\n{}", preview.iter().map(|f| format!("  {}", f)).collect::<Vec<_>>().join("\n"))
    } else {
        "No workspace mounted. You're running without a project folder.".to_string()
    };

    // Read key files
    let mut file_contexts = Vec::new();
    if has_workspace {
        let files = discover_files(&target_dir);
        let priority = ["README.md", "readme.md", "Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
        for file in &files {
            let basename = std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if priority.contains(&basename.as_str()) {
                if let Some(content) = read_file_safe(file, 6_000) {
                    file_contexts.push(format!("### {}\n```\n{}\n```", file.replace(&target_dir, "."), content));
                }
            }
        }
    }

    // Tool-use system prompt
    let tools_doc = r#"
## Available Tools
You can call tools by writing [TOOL:tool_name] followed by args and [/TOOL].

### read_file
Read a file from the workspace. Args: relative file path.
Example: [TOOL:read_file]src/main.rs[/TOOL]

### write_file
Write content to a file. Args: path, then ---CONTENT--- separator, then content.
Example: [TOOL:write_file]report.md
---CONTENT---
# My Report
Content here[/TOOL]

### list_files
List files in a directory. Args: directory path (empty = workspace root).
Example: [TOOL:list_files][/TOOL]

### shell
Run a shell command inside the container. Args: the command.
IMPORTANT: Always use absolute paths or `cd /workspace && command`.
Example: [TOOL:shell]cd /workspace && ls -la[/TOOL]

### browse
Open a URL in the browser (visible to the user in live view). Args: URL.
Example: [TOOL:browse]https://example.com[/TOOL]

### search_web
Search the web. Args: search query.
Example: [TOOL:search_web]rust async programming tutorial[/TOOL]

### delegate
Delegate a sub-task to a sub-agent that runs in parallel. Args: task description.
Use this to split complex tasks into smaller parts.
Example: [TOOL:delegate]Analyze all Python files for security issues[/TOOL]

## Response Format
- If you need a tool, use the tool syntax above. Only ONE tool per message.
- When you're done, respond with [DONE] and provide your final answer.
- Include [DONE] in your FINAL message with the complete answer.
- Structure your final answer in TWO parts separated by ---REPORT_SEPARATOR---:
  Part 1: Summary for chat (3-8 sentences, conversational, use markdown)
  Part 2: Detailed report (full markdown document)

## IMPORTANT
- If the user asks you a question, answer it directly — don't just use tools.
- Talk to the user naturally. Your responses will appear as chat messages.
- When using shell commands, always use absolute paths (prefix with /workspace/).
- If you need information from the user, ask clearly and wait for their response.
"#;

    let system_prompt = format!(
        "You are Sentinel, a personal AI agent running in a Docker container. \
        You can do anything the user asks: analyze files, browse the web, run commands, \
        write code, send emails, research topics, etc.\n\n\
        You can delegate sub-tasks to sub-agents using the [TOOL:delegate] tool.\n\n\
        Autonomy level: {}\n\n\
        ## Workspace\n{}\n\n\
        ## Key Files\n{}\n\n\
        {}\n",
        autonomy, workspace_overview,
        if file_contexts.is_empty() { "None read yet.".to_string() } else { file_contexts.join("\n\n") },
        tools_doc
    );

    // Tool-use conversation loop
    let mut messages = vec![
        ChatMessage { role: "system".into(), content: system_prompt },
        ChatMessage { role: "user".into(), content: task.clone() },
    ];

    let max_iterations = 20;
    for iteration in 0..max_iterations {
        host.log("info", "agent", &format!("THOUGHT: Waiting for LLM response from {}...", provider)).await;

        let response = match llm.chat(&messages).await {
            Ok(r) => r,
            Err(e) => {
                host.thought(&format!("❌ LLM error: {}", e)).await;
                break;
            }
        };

        // Check if the LLM is done
        if response.contains("[DONE]") {
            let final_text = response.replace("[DONE]", "").trim().to_string();

            // Split into summary + report
            let (summary, report_body) = if final_text.contains("---REPORT_SEPARATOR---") {
                let parts: Vec<&str> = final_text.splitn(2, "---REPORT_SEPARATOR---").collect();
                (parts[0].trim().to_string(), parts[1].trim().to_string())
            } else {
                // Use first ~8 lines as summary, full text as report
                let lines: Vec<&str> = final_text.lines().collect();
                let end = lines.len().min(8);
                (lines[..end].join("\n"), final_text.clone())
            };

            host.thought(&summary).await;

            // Write report
            if has_workspace {
                let report = format!(
                    "# Sentinel Agent Report\n\n**Task:** {}\n\n---\n\n## Summary\n\n{}\n\n---\n\n{}\n",
                    task, summary, report_body
                );
                let report_path = format!("{}/SENTINEL_REPORT.md", target_dir);
                match std::fs::write(&report_path, &report) {
                    Ok(_) => {
                        host.thought("✅ Full report written to `SENTINEL_REPORT.md`").await;
                    }
                    Err(e) => {
                        host.log("warn", "agent", &format!("Could not write report: {}", e)).await;
                        host.thought(&report_body).await;
                    }
                }
            } else {
                // No workspace — just send the full report in chat
                host.thought(&report_body).await;
            }
            break;
        }

        // Check for tool call
        if let Some((tool_name, tool_args)) = parse_tool_call(&response) {
            host.thought(&format!("Using tool: **{}**", tool_name)).await;

            if tool_name == "browse" || tool_name == "search_web" {
                host.gui_active(true).await;
            }

            let result = if tool_name == "delegate" {
                // Run a sub-agent
                let parent_ctx = format!("Main task: {}", task);
                run_subagent(&llm, &host, &tool_args, &target_dir, &parent_ctx).await
            } else {
                execute_tool(&tool_name, &tool_args, &target_dir)
            };

            host.log("info", "agent", &format!("Tool result ({}): {} chars", tool_name, result.len())).await;

            // Add to conversation
            messages.push(ChatMessage { role: "assistant".into(), content: response.clone() });
            messages.push(ChatMessage { role: "user".into(), content: format!("[Tool Result for {}]\n{}", tool_name, result) });
        } else {
            // No tool call — this is natural language from the agent (question or statement)
            let clean = response.trim();
            if !clean.is_empty() {
                host.thought(clean).await;
            }
            messages.push(ChatMessage { role: "assistant".into(), content: response });
            // Give the agent a chance to continue or receive user input
            messages.push(ChatMessage {
                role: "user".into(),
                content: "Continue with the task. If you need more information, ask clearly. \
                         Use tools if needed, or respond with [DONE] and your final answer if finished.".to_string()
            });
        }

        if iteration == max_iterations - 1 {
            host.thought("⚠️ Reached maximum iterations. Wrapping up...").await;
        }
    }

    if use_gui {
        host.gui_active(false).await;
    }

    host.thought("Task complete. Send me a message if you need anything else!").await;
    host.status("completed", "Task completed").await;
    Ok(())
}
