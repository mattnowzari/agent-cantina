use anyhow::{Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};

use crate::config::Config;

pub struct EsClient {
    base_url: String,
    http: reqwest::Client,
}

impl EsClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let base_url = normalize_base_url(
            cfg.es_host
                .as_deref()
                .context("ES_HOST is required for indexing")?,
        );
        let api_key = cfg
            .api_key
            .as_deref()
            .context("API_KEY is required for indexing")?
            .to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let auth_value = format!("ApiKey {}", api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).context("invalid API_KEY value")?,
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(cfg.insecure_tls)
            .danger_accept_invalid_hostnames(cfg.insecure_tls)
            .build()?;

        Ok(Self { base_url, http })
    }

    pub async fn ensure_index(&self, index: &str) -> Result<()> {
        let url = format!("{}/{}", self.base_url, index);
        let resp = self.http.head(&url).send().await.context("failed to send request")?;
        if resp.status().is_success() {
            return Ok(());
        }
        if resp.status().as_u16() != 404 {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Elasticsearch API error {}: {}", status, text);
        }

        let body = index_mapping_body();
        let resp = self
            .http
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Elasticsearch API error {}: {}", status, text);
        }
        Ok(())
    }

    pub async fn index_conversation(&self, index: &str, id: &str, doc: serde_json::Value) -> Result<()> {
        let url = format!("{}/{}/_doc/{}?refresh=wait_for", self.base_url, index, id);
        let resp = self
            .http
            .put(url)
            .json(&doc)
            .send()
            .await
            .context("failed to send request")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Elasticsearch API error {}: {}", status, text);
        }
        Ok(())
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

fn index_mapping_body() -> serde_json::Value {
    // `semantic_text` supports optional `inference_id`; if none is configured on the cluster,
    // indexing will error. We keep the mapping simple and let the cluster decide defaults.
    serde_json::json!({
      "mappings": {
        "dynamic": false,
        "properties": {
          "conversation_id": { "type": "keyword" },
          "agent_id": { "type": "keyword" },
          "dumped_at": { "type": "date" },
          "prompts": { "type": "semantic_text" },
          "responses": { "type": "semantic_text" },
          "turns": {
            "type": "object",
            "dynamic": false,
            "properties": {
              "prompt": { "type": "text" },
              "response": { "type": "text" }
            }
          }
        }
      }
    })
}

