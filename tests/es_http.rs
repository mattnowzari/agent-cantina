use agent_cantina::config::Config;
use agent_cantina::es::EsClient;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg_for(server: &MockServer) -> Config {
    Config {
        kibana_url: None,
        es_host: Some(server.uri()),
        api_key: Some("k".to_string()),
        space: None,
        agent_id: "elastic-ai-agent".to_string(),
        insecure_tls: false,
    }
}

fn auth_header_value() -> &'static str {
    "ApiKey k"
}

#[tokio::test]
async fn ensure_index_puts_mapping_when_missing() {
    let server = MockServer::start().await;

    // First, HEAD returns 404 (missing index).
    Mock::given(method("HEAD"))
        .and(path("/agent_cantina_conversations"))
        .and(header("authorization", auth_header_value()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    // Then, PUT creates index.
    Mock::given(method("PUT"))
        .and(path("/agent_cantina_conversations"))
        .and(header("authorization", auth_header_value()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"acknowledged": true})))
        .expect(1)
        .mount(&server)
        .await;

    let client = EsClient::new(&cfg_for(&server)).unwrap();
    client.ensure_index("agent_cantina_conversations").await.unwrap();
}

#[tokio::test]
async fn ensure_index_noops_when_present() {
    let server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/agent_cantina_conversations"))
        .and(header("authorization", auth_header_value()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let client = EsClient::new(&cfg_for(&server)).unwrap();
    client.ensure_index("agent_cantina_conversations").await.unwrap();
}

#[tokio::test]
async fn index_conversation_puts_doc_with_refresh() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/agent_cantina_conversations/_doc/conv-1"))
        .and(query_param("refresh", "wait_for"))
        .and(header("authorization", auth_header_value()))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"result": "created"})))
        .expect(1)
        .mount(&server)
        .await;

    let client = EsClient::new(&cfg_for(&server)).unwrap();
    client
        .index_conversation(
            "agent_cantina_conversations",
            "conv-1",
            json!({"conversation_id": "conv-1"}),
        )
        .await
        .unwrap();
}

