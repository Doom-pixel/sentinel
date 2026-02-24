//! # sentinel-host — LLM Provider Abstraction

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn, debug};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub timeout: Duration,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmProvider {
    Ollama {
        base_url: String,
    },
    OpenAi {
        api_key: String,
        org_id: Option<String>,
    },
    Anthropic {
        api_key: String,
    },
    Deepseek {
        api_key: String,
        base_url: Option<String>,
    },
    Grok {
        api_key: String,
    },
    Google {
        api_key: String,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub response_format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub usage: TokenUsage,
    pub model: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn health_check(&self) -> Result<bool>;
    fn provider_name(&self) -> &str;
}

pub struct OllamaBackend {
    pub base_url: String,
    pub model: String,
    pub config: LlmConfig,
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
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

        Ok(CompletionResponse {
            content: data["message"]["content"].as_str().unwrap_or_default().to_string(),
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
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "Ollama (Local)"
    }
}

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
        let choice = &data["choices"][0];
        let usage = &data["usage"];

        Ok(CompletionResponse {
            content: choice["message"]["content"].as_str().unwrap_or_default().to_string(),
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
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        &self.display_name
    }
}

pub struct AnthropicBackend {
    pub api_key: String,
    pub model: String,
    pub config: LlmConfig,
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
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
        let content = data["content"][0]["text"].as_str().unwrap_or_default().to_string();
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
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "Anthropic (Claude)"
    }
}

pub fn create_backend(config: &LlmConfig) -> Result<Box<dyn LlmBackend>> {
    let backend: Box<dyn LlmBackend> = match &config.provider {
        LlmProvider::Ollama { base_url } => {
            Box::new(OllamaBackend {
                base_url: base_url.clone(),
                model: config.model.clone(),
                config: config.clone(),
            })
        }
        LlmProvider::OpenAi { api_key, .. } => {
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://api.openai.com".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "OpenAI (ChatGPT)".into(),
            })
        }
        LlmProvider::Anthropic { api_key } => {
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
            Box::new(OpenAiCompatibleBackend {
                base_url: url,
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "Deepseek".into(),
            })
        }
        LlmProvider::Grok { api_key } => {
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://api.x.ai".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "xAI (Grok)".into(),
            })
        }
        LlmProvider::Google { api_key } => {
            Box::new(OpenAiCompatibleBackend {
                base_url: "https://generativelanguage.googleapis.com".into(),
                api_key: api_key.clone(),
                model: config.model.clone(),
                config: config.clone(),
                display_name: "Google (Gemini)".into(),
            })
        }
        LlmProvider::OpenAiCompatible { api_key, base_url } => {
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
