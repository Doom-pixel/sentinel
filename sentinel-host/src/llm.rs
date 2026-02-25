//! # sentinel-host — LLM Provider Abstraction
//!
//! Multi-backend LLM integration layer. The Guest's reasoning requests
//! are routed through this module, which supports:
//!
//! - **Local**: Ollama (any open-source model — Llama, Mistral, Qwen, etc.)
//! - **API**: OpenAI (ChatGPT), Anthropic (Claude), Deepseek, xAI (Grok),
//!   Google (Gemini), and a generic OpenAI-compatible endpoint.
//!
//! The Guest never knows which backend is active — it just sees the
//! `reasoning` WIT interface. Backend selection is a Host-side config concern.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn, debug};

// ─── Provider Configuration ─────────────────────────────────────────────────

/// Configuration for the active LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Which provider to use.
    pub provider: LlmProvider,
    /// Model identifier (e.g., "llama3.1:70b", "gpt-4o", "claude-sonnet-4-20250514").
    pub model: String,
    /// Maximum tokens in the response.
    pub max_tokens: u32,
    /// Temperature for sampling (0.0 = deterministic).
    pub temperature: f32,
    /// Request timeout.
    pub timeout: Duration,
    /// System prompt prepended to every request.
    pub system_prompt: Option<String>,
}

/// Supported LLM providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmProvider {
    /// Local Ollama instance (privacy-first, no data leaves the machine).
    Ollama {
        /// Ollama API base URL (default: http://localhost:11434).
        base_url: String,
    },
    /// OpenAI API (ChatGPT, GPT-4o, o1, etc.).
    OpenAi {
        api_key: String,
        /// Optional org ID.
        org_id: Option<String>,
    },
    /// Anthropic API (Claude).
    Anthropic {
        api_key: String,
    },
    /// Deepseek API.
    Deepseek {
        api_key: String,
        /// Base URL (default: https://api.deepseek.com).
        base_url: Option<String>,
    },
    /// xAI API (Grok).
    Grok {
        api_key: String,
    },
    /// Google Gemini API.
    Google {
        api_key: String,
    },
    /// Any OpenAI-compatible endpoint (LocalAI, vLLM, LM Studio, etc.).
    OpenAiCompatible {
        api_key: String,
        base_url: String,
    },
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Ollama {
                base_url: "http://localhost:11434".into(),
            },
            model: "llama3.1:8b".into(),
            max_tokens: 4096,
            temperature: 0.7,
            timeout: Duration::from_secs(120),
            system_prompt: Some(
                "You are SENTINEL, a secure autonomous agent. \
                 You must request capabilities through the host interface \
                 before accessing any resources."
                    .into(),
            ),
        }
    }
}

// ─── Message Types ──────────────────────────────────────────────────────────

/// A message in a conversation with the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Message role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A request to the LLM for reasoning/completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// Optional JSON schema for structured output.
    pub response_format: Option<serde_json::Value>,
}

/// The LLM's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The generated text.
    pub content: String,
    /// Token usage statistics.
    pub usage: TokenUsage,
    /// Which model actually served the request.
    pub model: String,
    /// Finish reason (e.g., "stop", "length").
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─── Provider Trait ─────────────────────────────────────────────────────────

/// Trait that all LLM providers implement.
///
/// This is the abstraction boundary — the Host engine calls `complete()`
/// regardless of whether the backend is local Ollama or a remote API.
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    /// Send a completion request and receive a response.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Check if the provider is reachable and the model is available.
    async fn health_check(&self) -> Result<bool>;

    /// Human-readable name for logging.
    fn provider_name(&self) -> &str;
}

// ─── Ollama Backend ─────────────────────────────────────────────────────────

/// Local Ollama backend — no data leaves the machine.
pub struct OllamaBackend {
    pub base_url: String,
    pub model: String,
    pub config: LlmConfig,
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        debug!(model = %self.model, "Ollama: sending completion request");

        // Build Ollama-native request payload
        let payload = serde_json::json!({
            "model": self.model,
            "messages": request.messages,
            "stream": false,
            "options": {
                "temperature": request.temperature.unwrap_or(self.config.temperature),
                "num_predict": request.max_tokens.unwrap_or(self.config.max_tokens),
            }
        });

        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()?;

        let res = client
            .post(format!("{}/api/chat", self.base_url))
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        let data: serde_json::Value = res.json().await?;

        let content = data["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Ollama error: {}", data))?
            .to_string();

        Ok(CompletionResponse {
            content,
            usage: TokenUsage {
                prompt_tokens: data["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
                completion_tokens: data["eval_count"].as_u64().unwrap_or(0) as u32,
                total_tokens: 0,
            },
            model: self.model.clone(),
            finish_reason: Some(data["done_reason"].as_str().unwrap_or("stop").to_string()),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // In production: GET {base_url}/api/tags and check model exists
        info!(base_url = %self.base_url, "Ollama health check (stub)");
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "Ollama (Local)"
    }
}

// ─── OpenAI-Compatible Backend ──────────────────────────────────────────────

/// Generic OpenAI-compatible backend. Works for:
/// - OpenAI (ChatGPT)
/// - Deepseek
/// - xAI (Grok)
/// - Google Gemini (via OpenAI compat endpoint)
/// - LocalAI, vLLM, LM Studio, etc.
pub struct OpenAiCompatibleBackend {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub config: LlmConfig,
    pub display_name: String,
}

#[async_trait::async_trait]
impl LlmBackend for OpenAiCompatibleBackend {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        debug!(model = %self.model, provider = %self.display_name,
               "Sending completion request");

        let mut payload = serde_json::json!({
            "model": self.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens.unwrap_or(self.config.max_tokens),
            "temperature": request.temperature.unwrap_or(self.config.temperature),
        });

        if let Some(format) = &request.response_format {
            payload["response_format"] = format.clone();
        }

        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()?;

        let res = client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        let data: serde_json::Value = res.json().await?;
        
        let choice = data.get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| anyhow::anyhow!("Invalid response from {}, raw JSON: {}", self.display_name, data))?;

        let content = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = &data["usage"];

        Ok(CompletionResponse {
            content,
            usage: TokenUsage {
                prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
            },
            model: self.model.clone(),
            finish_reason: Some(choice["finish_reason"].as_str().unwrap_or("stop").to_string()),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        info!(provider = %self.display_name, "Health check (stub)");
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        &self.display_name
    }
}

// ─── Anthropic Backend ──────────────────────────────────────────────────────

/// Anthropic Claude backend (uses the Messages API, not OpenAI-compat).
pub struct AnthropicBackend {
    pub api_key: String,
    pub model: String,
    pub config: LlmConfig,
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        debug!(model = %self.model, "Anthropic: sending completion request");

        // Anthropic uses a different message format:
        // - System prompt is a top-level field, not a message
        // - Only user/assistant messages in the messages array
        let system = request
            .messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.clone());

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .collect();

        let payload = serde_json::json!({
            "model": self.model,
            "max_tokens": request.max_tokens.unwrap_or(self.config.max_tokens),
            "system": system.unwrap_or_default(),
            "messages": messages,
        });

        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()?;

        let res = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        let data: serde_json::Value = res.json().await?;
        
        let content = data.get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid Anthropic response: {}", data))?
            .to_string();

        let usage = &data["usage"];

        Ok(CompletionResponse {
            content,
            usage: TokenUsage {
                prompt_tokens: usage["input_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: usage["output_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: 0,
            },
            model: self.model.clone(),
            finish_reason: Some(data["stop_reason"].as_str().unwrap_or("end_turn").to_string()),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        info!("Anthropic health check (stub)");
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "Anthropic (Claude)"
    }
}

// ─── Factory ────────────────────────────────────────────────────────────────

/// Create the appropriate LLM backend from configuration.
pub fn create_backend(config: &LlmConfig) -> Result<Box<dyn LlmBackend>> {
    let backend: Box<dyn LlmBackend> = match &config.provider {
        LlmProvider::Ollama { base_url } => {
            info!(model = %config.model, base_url = %base_url, "Using Ollama (local)");
            Box::new(OllamaBackend {
                base_url: base_url.clone(),
                model: config.model.clone(),
                config: config.clone(),
            })
        }
        LlmProvider::OpenAi { api_key, .. } => {
            info!(model = %config.model, "Using OpenAI");
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://api.openai.com".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "OpenAI (ChatGPT)".into(),
            })
        }
        LlmProvider::Anthropic { api_key } => {
            info!(model = %config.model, "Using Anthropic (Claude)");
            Box::new(AnthropicBackend {
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
            })
        }
        LlmProvider::Deepseek { api_key, base_url } => {
            let url = base_url
                .clone()
                .unwrap_or_else(|| "https://api.deepseek.com".into());
            info!(model = %config.model, "Using Deepseek");
            Box::new(OpenAiCompatibleBackend {
                base_url: url,
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "Deepseek".into(),
            })
        }
        LlmProvider::Grok { api_key } => {
            info!(model = %config.model, "Using xAI (Grok)");
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://api.x.ai".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "xAI (Grok)".into(),
            })
        }
        LlmProvider::Google { api_key } => {
            info!(model = %config.model, "Using Google (Gemini)");
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://generativelanguage.googleapis.com".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "Google (Gemini)".into(),
            })
        }
        LlmProvider::OpenAiCompatible { api_key, base_url } => {
            info!(model = %config.model, base_url = %base_url, "Using OpenAI-compatible endpoint");
            Box::new(OpenAiCompatibleBackend {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: format!("Custom ({})", base_url),
            })
        }
    };

    Ok(backend)
}
