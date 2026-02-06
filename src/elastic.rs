use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolSummary {
    pub id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "type", default)]
    pub tool_type: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub description: String,
}

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

    pub async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        let url = self.list_agents_url();
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {}: {}", status, text);
        }

        let v: serde_json::Value =
            serde_json::from_str(&text).context("failed to parse list agents response JSON")?;
        parse_agents(v).context("failed to parse agents from response")
    }

    fn list_agents_url(&self) -> String {
        match self.space.as_deref() {
            Some(space) => format!("{}/s/{}/api/agent_builder/agents", self.base_url, space),
            None => format!("{}/api/agent_builder/agents", self.base_url),
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolSummary>> {
        let url = self.list_tools_url();
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {}: {}", status, text);
        }

        let parsed: ListToolsResponse =
            serde_json::from_str(&text).context("failed to parse list tools response JSON")?;
        Ok(parsed.results)
    }

    fn list_tools_url(&self) -> String {
        match self.space.as_deref() {
            Some(space) => format!("{}/s/{}/api/agent_builder/tools", self.base_url, space),
            None => format!("{}/api/agent_builder/tools", self.base_url),
        }
    }

    pub async fn create_agent(&self, req: CreateAgentRequest) -> Result<AgentSummary> {
        let url = self.create_agent_url();
        let resp = self
            .http
            .post(url)
            .json(&req)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Agent Builder API error {}: {}", status, text);
        }

        let parsed: CreateAgentResponse =
            serde_json::from_str(&text).context("failed to parse create agent response JSON")?;

        Ok(AgentSummary {
            id: parsed.id,
            name: parsed.name,
            description: Some(parsed.description),
        })
    }

    fn create_agent_url(&self) -> String {
        match self.space.as_deref() {
            Some(space) => format!("{}/s/{}/api/agent_builder/agents", self.base_url, space),
            None => format!("{}/api/agent_builder/agents", self.base_url),
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

fn parse_agents(v: serde_json::Value) -> Result<Vec<AgentSummary>> {
    fn as_arr(v: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
        v.as_array()
    }

    // Try common response shapes: [..], { agents: [..] }, { data: [..] }, { items: [..] }
    let arr = if let Some(a) = as_arr(&v) {
        a
    } else if let Some(a) = v.get("agents").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("data").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("items").and_then(as_arr) {
        a
    } else if let Some(a) = v.get("results").and_then(as_arr) {
        a
    } else {
        anyhow::bail!("unexpected list agents JSON shape: {}", v);
    };

    let mut out = Vec::new();
    for item in arr {
        let obj = item.as_object().context("agent item is not an object")?;

        let id = obj
            .get("id")
            .or_else(|| obj.get("agent_id"))
            .or_else(|| obj.get("agentId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.trim().is_empty() {
            continue;
        }

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        out.push(AgentSummary {
            id,
            name,
            description,
        });
    }

    // Stable ordering for selection.
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
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

#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub steps: Vec<ConverseStep>,
}

#[derive(Debug, Deserialize)]
struct ListToolsResponse {
    #[serde(default)]
    results: Vec<ToolSummary>,
}

/// `POST /api/agent_builder/agents`
///
/// Docs:
/// - `https://www.elastic.co/docs/api/doc/kibana/operation/operation-post-agent-builder-agents`
#[derive(Debug, Serialize)]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub configuration: CreateAgentConfiguration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_symbol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAgentConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub tools: Vec<CreateAgentTools>,
}

#[derive(Debug, Serialize)]
pub struct CreateAgentTools {
    pub tool_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentResponse {
    id: String,
    name: String,
    description: String,
}
