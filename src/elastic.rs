use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AgentBuilderClient {
    base_url: String,
    agent_id: String,
    space: Option<String>,
    http: reqwest::Client,
}

impl AgentBuilderClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let base_url = normalize_base_url(
            cfg.kibana_url
                .as_deref()
                .context("KIBANA_URL (or ES_HOST) is required")?,
        );
        let api_key = cfg
            .api_key
            .as_deref()
            .context("API_KEY (or ES_API_KEY) is required")?
            .to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("kbn-xsrf", HeaderValue::from_static("true"));

        // Docs use: Authorization: ApiKey ${API_KEY}
        let auth_value = format!("ApiKey {}", api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).context("invalid API_KEY value")?,
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            base_url,
            agent_id: cfg.agent_id.clone(),
            space: cfg.space.clone(),
            http,
        })
    }

    pub async fn converse(
        &self,
        input: &str,
        conversation_id: Option<&str>,
    ) -> Result<ConverseResult> {
        let url = self.converse_url();

        let body = ConverseRequest {
            input,
            agent_id: &self.agent_id,
            conversation_id,
        };

        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {}: {}", status, text);
        }

        let parsed: ConverseResponse =
            serde_json::from_str(&text).context("failed to parse Agent Builder response JSON")?;

        Ok(ConverseResult {
            conversation_id: parsed.conversation_id,
            message: parsed.response.message,
            steps: parsed.steps,
        })
    }

    fn converse_url(&self) -> String {
        match self.space.as_deref() {
            Some(space) => format!("{}/s/{}/api/agent_builder/converse", self.base_url, space),
            None => format!("{}/api/agent_builder/converse", self.base_url),
        }
    }
}

fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    }
}

#[derive(Debug, Serialize)]
struct ConverseRequest<'a> {
    input: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ConverseResponse {
    #[serde(default)]
    steps: Vec<ConverseStep>,
    response: ConverseResponseMessage,
    conversation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConverseResponseMessage {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConverseStep {
    Reasoning {
        reasoning: String,
        #[serde(default)]
        transient: bool,
    },
    ToolCall {
        tool_call_id: String,
        tool_id: String,
        #[serde(default)]
        params: serde_json::Value,
        #[serde(default)]
        progression: Vec<serde_json::Value>,
        #[serde(default)]
        results: Vec<serde_json::Value>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone)]
pub struct ConverseResult {
    pub conversation_id: Option<String>,
    pub message: String,
    pub steps: Vec<ConverseStep>,
}
