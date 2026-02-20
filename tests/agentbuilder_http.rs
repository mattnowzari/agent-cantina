use agent_cantina::agentbuilder::{
    AgentBuilderClient, CreateAgentConfiguration, CreateAgentRequest, CreateAgentTools,
    UpdateAgentRequest,
};
use agent_cantina::config::Config;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg_for(server: &MockServer) -> Config {
    Config {
        kibana_url: Some(server.uri()),
        es_host: None,
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
async fn list_agents_parses_results_shape() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/agent_builder/agents"))
        .and(header("authorization", auth_header_value()))
        .and(header("kbn-xsrf", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [
                {
                    "id": "a-1",
                    "name": "Agent 1",
                    "description": "desc",
                    "configuration": {
                        "instructions": "do things",
                        "tools": [{ "tool_ids": ["t1", "t2"] }]
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = AgentBuilderClient::new(&cfg_for(&server)).unwrap();
    let agents = client.list_agents().await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "a-1");
    assert_eq!(agents[0].name, "Agent 1");
    assert_eq!(agents[0].instructions.as_deref(), Some("do things"));
    assert_eq!(agents[0].tool_ids, vec!["t1".to_string(), "t2".to_string()]);
}

#[tokio::test]
async fn list_tools_parses_results() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/agent_builder/tools"))
        .and(header("authorization", auth_header_value()))
        .and(header("kbn-xsrf", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [
                {
                    "id": "tool-1",
                    "tags": ["tag1"],
                    "type": "platform",
                    "readonly": true,
                    "description": "A tool"
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = AgentBuilderClient::new(&cfg_for(&server)).unwrap();
    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].id, "tool-1");
    assert_eq!(tools[0].tool_type, "platform");
    assert!(tools[0].readonly);
}

#[tokio::test]
async fn create_agent_sends_expected_payload_and_returns_summary() {
    let server = MockServer::start().await;

    let req = CreateAgentRequest {
        id: "a-1".to_string(),
        name: "Agent 1".to_string(),
        description: "desc".to_string(),
        configuration: CreateAgentConfiguration {
            instructions: Some("do things".to_string()),
            tools: vec![CreateAgentTools {
                tool_ids: vec!["t1".to_string()],
            }],
        },
        avatar_color: None,
        avatar_symbol: None,
        labels: vec![],
    };

    Mock::given(method("POST"))
        .and(path("/api/agent_builder/agents"))
        .and(header("authorization", auth_header_value()))
        .and(header("kbn-xsrf", "true"))
        .and(body_json(&json!({
            "id": "a-1",
            "name": "Agent 1",
            "description": "desc",
            "configuration": {
                "instructions": "do things",
                "tools": [{ "tool_ids": ["t1"] }]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "a-1",
            "name": "Agent 1",
            "description": "desc"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = AgentBuilderClient::new(&cfg_for(&server)).unwrap();
    let created = client.create_agent(req).await.unwrap();
    assert_eq!(created.id, "a-1");
    assert_eq!(created.instructions.as_deref(), Some("do things"));
    assert_eq!(created.tool_ids, vec!["t1".to_string()]);
}

#[tokio::test]
async fn update_agent_sends_expected_payload_and_returns_summary() {
    let server = MockServer::start().await;

    let req = UpdateAgentRequest {
        name: "Agent 1".to_string(),
        description: "desc".to_string(),
        configuration: CreateAgentConfiguration {
            instructions: Some("updated".to_string()),
            tools: vec![CreateAgentTools {
                tool_ids: vec!["t1".to_string(), "t2".to_string()],
            }],
        },
        avatar_color: None,
        avatar_symbol: None,
        labels: vec![],
    };

    Mock::given(method("PUT"))
        .and(path("/api/agent_builder/agents/a-1"))
        .and(header("authorization", auth_header_value()))
        .and(header("kbn-xsrf", "true"))
        .and(body_json(&json!({
            "name": "Agent 1",
            "description": "desc",
            "configuration": {
                "instructions": "updated",
                "tools": [{ "tool_ids": ["t1", "t2"] }]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "a-1",
            "name": "Agent 1",
            "description": "desc"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = AgentBuilderClient::new(&cfg_for(&server)).unwrap();
    let updated = client.update_agent("a-1", req).await.unwrap();
    assert_eq!(updated.id, "a-1");
    assert_eq!(updated.instructions.as_deref(), Some("updated"));
    assert_eq!(
        updated.tool_ids,
        vec!["t1".to_string(), "t2".to_string()]
    );
}

#[tokio::test]
async fn delete_agent_calls_delete_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/api/agent_builder/agents/a-1"))
        .and(header("authorization", auth_header_value()))
        .and(header("kbn-xsrf", "true"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let client = AgentBuilderClient::new(&cfg_for(&server)).unwrap();
    client.delete_agent("a-1").await.unwrap();
}

