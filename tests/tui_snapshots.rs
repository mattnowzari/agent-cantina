use agent_cantina::agentbuilder::AgentSummary;
use agent_cantina::config::Config;
use agent_cantina::elm::{
    ActivePanel, AgentEditorMode, ChatEntry, ChatRole, ConfirmDeleteAgentModal, CreateAgentModal,
    CreateAgentTab, Modal, Model,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
    let w = buf.area.width;
    let h = buf.area.height;
    let mut lines: Vec<String> = Vec::with_capacity(h as usize);
    for y in 0..h {
        let mut line = String::new();
        for x in 0..w {
            line.push_str(buf[(x, y)].symbol());
        }
        // Trim trailing whitespace to keep snapshots compact.
        let trimmed = line.trim_end_matches(' ').to_string();
        lines.push(trimmed);
    }
    // Drop trailing empty lines to keep snapshots stable across terminal sizes.
    while matches!(lines.last(), Some(s) if s.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn render_model(model: &mut Model, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| agent_cantina::elm::view(frame, model))
        .unwrap();
    let buf = terminal.backend().buffer();
    buffer_to_string(buf)
}

fn ready_config() -> Config {
    Config {
        kibana_url: Some("http://127.0.0.1:5601".to_string()),
        es_host: None,
        api_key: Some("k".to_string()),
        space: None,
        agent_id: "elastic-ai-agent".to_string(),
        insecure_tls: false,
    }
}

fn base_ready_model() -> Model {
    let mut model = Model::default();
    model.config = ready_config();
    model.env_loaded = true;
    model.prompts_loaded = true;
    model.prompts_path = "PROMPTS.md".to_string();
    model.prompts_raw = "## Prompt 1\nWhat is Elasticsearch?\n\n## Prompt 2\nWhat is Kibana?\n".to_string();
    model.prompts = vec!["What is Elasticsearch?".to_string(), "What is Kibana?".to_string()];

    model.agents_loaded = true;
    model.agents_loading = false;
    model.agents = vec![
        AgentSummary {
            id: "a-1".to_string(),
            name: "Researcher".to_string(),
            description: Some("Finds things".to_string()),
            instructions: Some("Be helpful".to_string()),
            tool_ids: vec!["t1".to_string()],
        },
        AgentSummary {
            id: "a-2".to_string(),
            name: "Writer".to_string(),
            description: None,
            instructions: None,
            tool_ids: vec![],
        },
    ];
    model.agent_selected_index = 0;

    model.chat = vec![
        ChatEntry {
            role: ChatRole::User,
            timestamp: "12:00:00".to_string(),
            content: "Hello".to_string(),
        },
        ChatEntry {
            role: ChatRole::Agent,
            timestamp: "12:00:01".to_string(),
            content: "Hi!".to_string(),
        },
    ];
    model
}

#[test]
fn snapshot_base_layout() {
    let mut model = base_ready_model();
    model.active = ActivePanel::Top;
    insta::assert_snapshot!(render_model(&mut model, 70, 22));
}

#[test]
fn snapshot_create_agent_modal() {
    let mut model = base_ready_model();
    let mut modal = CreateAgentModal::default();
    modal.mode = AgentEditorMode::Create;
    modal.tab = CreateAgentTab::Prompt;
    modal.name = "My agent".to_string();
    modal.description = "Desc".to_string();
    modal.instructions = "Do a thing".to_string();
    model.modal = Some(Modal::CreateAgent(modal));
    insta::assert_snapshot!(render_model(&mut model, 70, 22));
}

#[test]
fn snapshot_confirm_delete_modal() {
    let mut model = base_ready_model();
    model.modal = Some(Modal::ConfirmDeleteAgent(ConfirmDeleteAgentModal {
        agent_id: "a-1".to_string(),
        agent_name: "Researcher".to_string(),
        deleting: false,
    }));
    insta::assert_snapshot!(render_model(&mut model, 70, 22));
}

#[test]
fn snapshot_conversation_bubbles_with_emoji() {
    let mut model = base_ready_model();
    model.active = ActivePanel::Bottom;
    model.chat = vec![
        ChatEntry {
            role: ChatRole::User,
            timestamp: "12:00:00".to_string(),
            content: "⚔️  crossed swords".to_string(),
        },
        ChatEntry {
            role: ChatRole::Agent,
            timestamp: "12:00:01".to_string(),
            content: "🌊🏄‍♂️☀️  surf vibes".to_string(),
        },
    ];
    // Render into a taller terminal so the conversation pane has enough height
    // to show bubble bodies (with 70x22, the bottom pane is only a few rows tall).
    let out = render_model(&mut model, 70, 40);

    // Sanity check: bubble borders + the message text should be visible.
    assert!(out.contains('┌') && out.contains('┐'));
    assert!(out.contains('│') && out.contains('└') && out.contains('┘'));
    assert!(out.contains("crossed swords"));
    assert!(out.contains("surf vibes"));
}

