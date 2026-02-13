use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use super::{
    Cmd, Model, Msg,
    model::{ActivePanel, AgentEditorMode, CreateAgentField, CreateAgentModal, CreateAgentTab, Modal},
};

pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    // When a modal is open, keyboard input goes to the modal, but we still
    // allow background messages (e.g. AgentCreated) to be processed.
    if model.modal.is_some() {
        match msg {
            Msg::Quit => {
                model.should_quit = true;
                return vec![];
            }
            Msg::Key(key) => return update_modal_key(model, key),
            _ => {}
        }
    }

    match msg {
        Msg::Init => vec![
            Cmd::LoadPromptsFile {
                path: model.prompts_path.clone(),
            },
            Cmd::LoadEnv,
        ],

        Msg::Quit => {
            model.should_quit = true;
            vec![]
        }

        Msg::Key(key) => {
            // Prompts editor: capture most keys when Prompts panel is active.
            if model.active == ActivePanel::Top {
                if let Some(cmds) = handle_prompts_editor_key(model, key) {
                    return cmds;
                }
            }
            // Standardized reload: Ctrl+R in Agents panel reloads agents list.
            if model.active == ActivePanel::Agents
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('r')
            {
                model.agents_loaded = false;
                model.agents_loading = false;
                model.agents_error = None;
                model.agents.clear();
                model.agent_selected_index = 0;
                model.selected_agent_id = None;
                model.agents_list_state = ratatui::widgets::ListState::default();
                return maybe_load_agents(model);
            }
            // Conversation dump: Ctrl+S when Conversation panel is active.
            if model.active == ActivePanel::Bottom
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('s')
            {
                let (path, md) = build_conversation_markdown_dump(model);
                return vec![Cmd::DumpConversationMarkdown {
                    path,
                    markdown: md,
                }];
            }
            // Conversation index:
            // - Prefer plain `i` because Ctrl+I is indistinguishable from Tab in many terminals.
            // - Also accept Ctrl+I if a terminal reports it distinctly.
            let index_shortcut = model.active == ActivePanel::Bottom
                && (key.code == KeyCode::Char('i')
                    || key.code == KeyCode::Char('I')
                    || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('i')));
            if index_shortcut {
                let Some(conversation_id) = model.conversation_id.clone() else {
                    model.modal = Some(Modal::Info {
                        title: "No conversation id".to_string(),
                        message: "Run a chat session first so we have a conversation_id to index under."
                            .to_string(),
                    });
                    return vec![];
                };
                if model.config.es_host.as_deref().unwrap_or("").is_empty() {
                    model.modal = Some(Modal::Info {
                        title: "Missing ES_HOST".to_string(),
                        message: "Set ES_HOST (optional) to enable indexing chat history to Elasticsearch."
                            .to_string(),
                    });
                    return vec![];
                }

                model.indexing_conversation = true;
                let (index, id, doc) = build_conversation_es_doc(model, &conversation_id);
                return vec![Cmd::IndexConversationToEs { index, id, doc }];
            }
            match key.code {
                // Panel selection
                KeyCode::Tab => cycle_panel(model, CycleDir::Forward),
                KeyCode::BackTab => cycle_panel(model, CycleDir::Backward),
                KeyCode::Esc => {
                    model.should_quit = true;
                }

                // Scrolling
                KeyCode::Up | KeyCode::Char('k') => scroll_or_select(model, ScrollDir::Up, 3),
                KeyCode::Down | KeyCode::Char('j') => scroll_or_select(model, ScrollDir::Down, 3),
                KeyCode::PageUp => scroll_or_select(model, ScrollDir::Up, 10),
                KeyCode::PageDown => scroll_or_select(model, ScrollDir::Down, 10),
                KeyCode::End => {
                    if model.active == ActivePanel::Bottom {
                        model.chat_scroll_from_bottom = 0;
                    }
                }

                // Select agent + run
                KeyCode::Enter => {
                    if model.active == ActivePanel::Agents {
                        if model.agents.is_empty() {
                            model.modal = Some(super::model::Modal::Info {
                                title: "No agents".to_string(),
                                message: "No agents were returned by Agent Builder.".to_string(),
                            });
                            return vec![];
                        }

                        let idx = model.agent_selected_index.min(model.agents.len() - 1);
                        let agent = model.agents[idx].clone();
                        model.selected_agent_id = Some(agent.id.clone());
                        model.config.agent_id = agent.id.clone();
                        model.chat.push(super::model::ChatEntry::system(format!(
                            "Selected agent: {} ({})",
                            agent.name, agent.id
                        )));
                        model.active = ActivePanel::Bottom;

                        if model.run_state != super::model::RunState::Running
                            && model.config.is_ready()
                            && !model.prompts.is_empty()
                        {
                            model.run_state = super::model::RunState::Running;
                            return vec![Cmd::StartRun];
                        }
                    }
                }

                // Create a new agent
                KeyCode::Char('n') => {
                    if model.active == ActivePanel::Agents {
                        if !model.config.is_ready() {
                            model.modal = Some(Modal::Info {
                                title: "Missing env".to_string(),
                                message:
                                    "Set KIBANA_URL/ES_HOST and API_KEY/ES_API_KEY before creating an agent."
                                        .to_string(),
                            });
                            return vec![];
                        }
                        let mut state = CreateAgentModal::default();
                        state.tools_loading = true;
                        model.modal = Some(Modal::CreateAgent(state));
                        // Kick off tool discovery immediately (for the Tools tab).
                        return vec![Cmd::LoadTools];
                    }
                }
                // Edit selected agent
                KeyCode::Char('e') => {
                    if model.active == ActivePanel::Agents {
                        if model.agents.is_empty() {
                            return vec![];
                        }
                        let idx = model.agent_selected_index.min(model.agents.len() - 1);
                        let agent = model.agents[idx].clone();

                        let mut state = CreateAgentModal::default();
                        state.mode = AgentEditorMode::Edit {
                            agent_id: agent.id.clone(),
                        };
                        state.name = agent.name;
                        state.description = agent.description.unwrap_or_default();
                        state.instructions = agent.instructions.unwrap_or_default();
                        state.selected_tool_ids = agent.tool_ids;
                        if state.selected_tool_ids.is_empty() {
                            // Fallback to defaults if the agent has no tools.
                            state.selected_tool_ids = CreateAgentModal::default().selected_tool_ids;
                        }
                        state.tools_loading = true;
                        state.tab = CreateAgentTab::Prompt;
                        state.focus = CreateAgentField::Name;

                        model.modal = Some(Modal::CreateAgent(state));
                        return vec![Cmd::LoadTools];
                    }
                }

                // Re-run prompts
                KeyCode::Char('r') => {
                    if model.run_state != super::model::RunState::Running {
                        if model.selected_agent_id.is_none() {
                            model.modal = Some(super::model::Modal::Info {
                                title: "Select an agent".to_string(),
                                message: "Pick an agent in the Agents window (↑/↓ + Enter) before running prompts."
                                    .to_string(),
                            });
                            model.active = ActivePanel::Agents;
                            return vec![];
                        }
                        model.run_state = super::model::RunState::Running;
                        return vec![Cmd::StartRun];
                    }
                }

                _ => {}
            }
            vec![]
        }
        Msg::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => scroll_or_select(model, ScrollDir::Up, 3),
                MouseEventKind::ScrollDown => scroll_or_select(model, ScrollDir::Down, 3),
                _ => {}
            }
            vec![]
        }

        Msg::Tick => {
            if model.waiting_for_response || model.indexing_conversation {
                model.spinner_frame = model.spinner_frame.wrapping_add(1);
            }
            vec![]
        }
        Msg::Resize => vec![],

        Msg::PromptsLoaded { raw, prompts } => {
            model.prompts_loaded = true;
            model.prompts_raw = raw;
            model.prompts = prompts;
            model.prompts_scroll = 0;
            model.prompts_cursor = model.prompts_raw.len();

            if model.prompts.is_empty() {
                model.chat.push(super::model::ChatEntry::system(
                    "PROMPTS.md loaded but no prompts were parsed.",
                ));
            } else {
                model.chat.push(super::model::ChatEntry::system(format!(
                    "Loaded {} prompt(s) from {}.",
                    model.prompts.len(),
                    model.prompts_path
                )));
            }

            maybe_load_agents(model)
        }

        Msg::PromptsLoadFailed { error } => {
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to load PROMPTS.md".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::PromptsSaved { raw, prompts } => {
            model.prompts_loaded = true;
            model.prompts_raw = raw;
            model.prompts = prompts;
            model.chat.push(super::model::ChatEntry::system("Saved PROMPTS.md."));
            // Keep cursor in range.
            model.prompts_cursor = model.prompts_cursor.min(model.prompts_raw.len());
            vec![]
        }

        Msg::PromptsSaveFailed { error } => {
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to save PROMPTS.md".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::ConversationDumped { path } => {
            model.chat.push(super::model::ChatEntry::system(format!(
                "Conversation dumped to {path}"
            )));
            vec![]
        }

        Msg::ConversationDumpFailed { error } => {
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to dump conversation".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::ConversationIndexed { index, id } => {
            model.indexing_conversation = false;
            model.spinner_frame = 0;
            model.chat.push(super::model::ChatEntry::system(format!(
                "Indexed conversation to Elasticsearch: index={index}, id={id}"
            )));
            vec![]
        }

        Msg::ConversationIndexFailed { error } => {
            model.indexing_conversation = false;
            model.spinner_frame = 0;
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to index conversation".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::EnvLoaded { config } => {
            model.env_loaded = true;
            model.config = config;
            let missing = model.config.missing();
            if !missing.is_empty() {
                model.modal = Some(super::model::Modal::MissingEnv { missing });
                model.chat.push(super::model::ChatEntry::system(
                    "Missing required env vars; not running prompts yet.",
                ));
                return vec![];
            }

            let host = model
                .config
                .kibana_url
                .as_deref()
                .unwrap_or("<missing>")
                .to_string();
            let api_key = if model.config.api_key.as_deref().unwrap_or("").is_empty() {
                "missing"
            } else {
                "set"
            };
            model.chat.push(super::model::ChatEntry::system(format!(
                "Detected env: host={host}, api_key={api_key}, space={}, agent_id={}",
                model.config.space.as_deref().unwrap_or("<default>"),
                model.config.agent_id
            )));

            model.chat.push(super::model::ChatEntry::system(
                "Env loaded (KIBANA_URL/ES_HOST, API_KEY/ES_API_KEY).",
            ));

            maybe_load_agents(model)
        }

        Msg::AgentsLoaded { agents } => {
            model.agents_loading = false;
            model.agents_loaded = true;
            model.agents_error = None;
            model.agents = agents;
            model.active = ActivePanel::Agents;

            if model.agents.is_empty() {
                model.modal = Some(super::model::Modal::Error {
                    title: "No agents found".to_string(),
                    message: "Agent Builder returned 0 agents.".to_string(),
                });
                return vec![];
            }

            // Preselect the agent from env/config if present.
            if let Some(idx) = model
                .agents
                .iter()
                .position(|a| a.id == model.config.agent_id)
            {
                model.agent_selected_index = idx;
            } else {
                model.agent_selected_index = 0;
            }
            model
                .agents_list_state
                .select(Some(model.agent_selected_index.min(model.agents.len() - 1)));

            model.chat.push(super::model::ChatEntry::system(format!(
                "Loaded {} agent(s). Select one (↑/↓ + Enter).",
                model.agents.len()
            )));

            vec![]
        }

        Msg::AgentsLoadFailed { error } => {
            model.agents_loading = false;
            model.agents_loaded = false;
            model.agents_error = Some(error.clone());
            // If env is missing, prefer the MissingEnv modal over a generic request failure.
            let missing = model.config.missing();
            if !missing.is_empty() {
                model.modal = Some(super::model::Modal::MissingEnv { missing });
            } else {
                model.modal = Some(super::model::Modal::Error {
                    title: "Failed to load agents".to_string(),
                    message: error,
                });
            }
            vec![]
        }

        Msg::ToolsLoaded { mut tools } => {
            // Stable ordering for keyboard navigation.
            tools.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
            if let Some(Modal::CreateAgent(state)) = model.modal.as_mut() {
                state.tools_loading = false;
                state.tools_error = None;
                state.tools = tools;
                state.tools_selected_index = 0;
                state.tools_list_state = ratatui::widgets::ListState::default();
                if !state.tools.is_empty() {
                    state.tools_list_state.select(Some(0));
                }
                // Prune selected tool IDs to those that exist (keeps payload valid).
                let valid: std::collections::HashSet<String> =
                    state.tools.iter().map(|t| t.id.clone()).collect();
                state.selected_tool_ids.retain(|id| valid.contains(id));
            }
            vec![]
        }

        Msg::ToolsLoadFailed { error } => {
            if let Some(Modal::CreateAgent(state)) = model.modal.as_mut() {
                state.tools_loading = false;
                state.tools_error = Some(error.clone());
            } else {
                // If env is missing, show the MissingEnv modal; otherwise log it.
                let missing = model.config.missing();
                if !missing.is_empty() {
                    model.modal = Some(super::model::Modal::MissingEnv { missing });
                } else {
                    model.chat.push(super::model::ChatEntry::system(format!(
                        "Failed to load tools: {}",
                        error
                    )));
                }
            }
            vec![]
        }

        Msg::AgentUpserted { agent, is_edit } => {
            model.chat.push(super::model::ChatEntry::system(format!(
                "{} agent: {} ({})",
                if is_edit { "Updated" } else { "Created" },
                agent.name, agent.id
            )));
            // Close the create-agent modal if it was open.
            if matches!(model.modal, Some(Modal::CreateAgent(_))) {
                model.modal = None;
            }
            model.selected_agent_id = Some(agent.id.clone());
            model.config.agent_id = agent.id.clone();

            // Reload list so it shows up immediately.
            model.agents_loaded = false;
            model.agents_loading = false;
            model.agents_error = None;
            model.agents.clear();
            model.agent_selected_index = 0;
            model.agents_list_state = ratatui::widgets::ListState::default();
            model.active = ActivePanel::Agents;
            vec![Cmd::LoadAgents]
        }

        Msg::AgentUpsertFailed { error, is_edit } => {
            model.chat.push(super::model::ChatEntry::system(format!(
                "{} agent failed: {}",
                if is_edit { "Update" } else { "Create" },
                error
            )));
            // If the create-agent modal is open, show the error inline there.
            if let Some(Modal::CreateAgent(state)) = model.modal.as_mut() {
                state.submitting = false;
                state.error = Some(error);
            } else {
                model.modal = Some(Modal::Error {
                    title: if is_edit {
                        "Failed to update agent".to_string()
                    } else {
                        "Failed to create agent".to_string()
                    },
                    message: error,
                });
            }
            vec![]
        }

        Msg::AppendChat(entry) => {
            model.chat.push(entry);
            vec![]
        }

        Msg::SetWaiting(waiting) => {
            model.waiting_for_response = waiting;
            if !waiting {
                model.spinner_frame = 0;
            }
            vec![]
        }

        Msg::SetConversationId(id) => {
            model.conversation_id = id;
            vec![]
        }

        Msg::RunStarted => {
            model.run_state = super::model::RunState::Running;
            model.waiting_for_response = false;
            model.spinner_frame = 0;
            model.chat.push(super::model::ChatEntry::system(
                "Running prompts… (press 'q' to quit)",
            ));
            vec![]
        }

        Msg::RunFinished => {
            model.run_state = super::model::RunState::Done;
            model.waiting_for_response = false;
            model
                .chat
                .push(super::model::ChatEntry::system("Run complete."));
            vec![]
        }

        Msg::RunFailed { error } => {
            model.run_state = super::model::RunState::Error;
            model.waiting_for_response = false;
            model.modal = Some(super::model::Modal::Error {
                title: "Agent Builder request failed".to_string(),
                message: error.clone(),
            });
            model.chat.push(super::model::ChatEntry::system(format!(
                "Run failed: {}",
                error
            )));
            vec![]
        }

    }
}

fn build_conversation_es_doc(model: &Model, conversation_id: &str) -> (String, String, serde_json::Value) {
    const INDEX: &str = "agent_cantina_conversations";

    let agent_id = model
        .selected_agent_id
        .as_deref()
        .unwrap_or(&model.config.agent_id)
        .to_string();

    let mut prompts: Vec<String> = Vec::new();
    let mut responses: Vec<String> = Vec::new();
    let mut turns: Vec<serde_json::Value> = Vec::new();

    let mut pending_prompt: Option<String> = None;
    for entry in &model.chat {
        match entry.role {
            super::model::ChatRole::System => {}
            super::model::ChatRole::User => {
                prompts.push(entry.content.clone());
                pending_prompt = Some(entry.content.clone());
            }
            super::model::ChatRole::Agent => {
                responses.push(entry.content.clone());
                let prompt = pending_prompt.take().unwrap_or_default();
                turns.push(serde_json::json!({
                    "prompt": prompt,
                    "response": entry.content
                }));
            }
        }
    }

    let doc = serde_json::json!({
      "conversation_id": conversation_id,
      "agent_id": agent_id,
      "dumped_at": chrono::Local::now().to_rfc3339(),
      "prompts": prompts.join("\n\n---\n\n"),
      "responses": responses.join("\n\n---\n\n"),
      "turns": turns
    });

    (INDEX.to_string(), conversation_id.to_string(), doc)
}

fn build_conversation_markdown_dump(model: &Model) -> (String, String) {
    let dumped_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let fname_ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let path = format!("conversation_dump_{fname_ts}.md");

    let agent_id = model
        .selected_agent_id
        .as_deref()
        .unwrap_or(&model.config.agent_id);
    let conversation_id = model
        .conversation_id
        .as_deref()
        .unwrap_or("<none>");

    let mut out = String::new();
    out.push_str("# Agent Cantina conversation dump\n\n");
    out.push_str(&format!("- dumped_at: `{dumped_at}`\n"));
    out.push_str(&format!("- agent_id: `{agent_id}`\n"));
    out.push_str(&format!("- conversation_id: `{conversation_id}`\n"));
    out.push('\n');
    out.push_str("---\n\n");

    for entry in &model.chat {
        let who = match entry.role {
            super::model::ChatRole::System => "system",
            super::model::ChatRole::User => "you",
            super::model::ChatRole::Agent => "agent",
        };
        out.push_str(&format!("## [{}] {who}\n\n", entry.timestamp));

        let fence = markdown_fence(&entry.content);
        out.push_str(&fence);
        out.push('\n');
        out.push_str("text\n");
        out.push_str(&entry.content);
        if !entry.content.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&fence);
        out.push('\n');
        out.push('\n');
        out.push('\n');
    }

    (path, out)
}

fn markdown_fence(s: &str) -> String {
    // Pick a backtick fence longer than any run of backticks in content.
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for ch in s.chars() {
        if ch == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    let n = (max_run + 1).max(3);
    "`".repeat(n)
}

fn handle_prompts_editor_key(model: &mut Model, key: ratatui::crossterm::event::KeyEvent) -> Option<Vec<Cmd>> {
    // Ctrl+S saves to disk.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        return Some(vec![Cmd::SavePromptsFile {
            path: model.prompts_path.clone(),
            raw: model.prompts_raw.clone(),
        }]);
    }
    // Ctrl+R reloads from disk.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
        return Some(vec![Cmd::LoadPromptsFile {
            path: model.prompts_path.clone(),
        }]);
    }

    // Don't treat Alt/Ctrl modified chars as text entry.
    if key.modifiers.contains(KeyModifiers::ALT) {
        return Some(vec![]);
    }

    match key.code {
        KeyCode::Left => {
            model.prompts_cursor = prev_char_boundary(&model.prompts_raw, model.prompts_cursor);
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        KeyCode::Right => {
            model.prompts_cursor = next_char_boundary(&model.prompts_raw, model.prompts_cursor);
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        KeyCode::Up => {
            model.prompts_cursor = move_cursor_vertical(&model.prompts_raw, model.prompts_cursor, -1);
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        KeyCode::Down => {
            model.prompts_cursor = move_cursor_vertical(&model.prompts_raw, model.prompts_cursor, 1);
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        KeyCode::Backspace => {
            if model.prompts_cursor > 0 {
                let prev = prev_char_boundary(&model.prompts_raw, model.prompts_cursor);
                model.prompts_raw.replace_range(prev..model.prompts_cursor, "");
                model.prompts_cursor = prev;
                // Re-parse for run readiness without requiring a save.
                model.prompts = crate::prompts::parse_prompts_markdown(model.prompts_raw.clone()).prompts;
                ensure_prompts_cursor_visible(model);
            }
            Some(vec![])
        }
        KeyCode::Enter => {
            let idx = model.prompts_cursor.min(model.prompts_raw.len());
            model.prompts_raw.insert(idx, '\n');
            model.prompts_cursor = idx + 1;
            model.prompts = crate::prompts::parse_prompts_markdown(model.prompts_raw.clone()).prompts;
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return None; // let global handler see Ctrl combos we didn't consume
            }
            let idx = model.prompts_cursor.min(model.prompts_raw.len());
            model.prompts_raw.insert(idx, c);
            model.prompts_cursor = idx + c.len_utf8();
            model.prompts = crate::prompts::parse_prompts_markdown(model.prompts_raw.clone()).prompts;
            ensure_prompts_cursor_visible(model);
            Some(vec![])
        }
        _ => None,
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    s[..idx].char_indices().last().map(|(i, _)| i).unwrap_or(0)
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut iter = s[idx..].char_indices();
    let _ = iter.next();
    match iter.next() {
        Some((off, _)) => idx + off,
        None => s.len(),
    }
}

fn move_cursor_vertical(s: &str, idx: usize, dir: i32) -> usize {
    let idx = idx.min(s.len());
    let before = &s[..idx];
    let line = before.as_bytes().iter().filter(|&&b| b == b'\n').count();
    let line_starts = line_start_bytes(s);
    if dir < 0 {
        if line == 0 {
            return idx;
        }
        let target_line = line - 1;
        let col = col_in_line(s, idx, line_starts[line]);
        return byte_index_for_line_col(s, &line_starts, target_line, col);
    }
    // down
    if line + 1 >= line_starts.len() {
        return idx;
    }
    let target_line = line + 1;
    let col = col_in_line(s, idx, line_starts[line]);
    byte_index_for_line_col(s, &line_starts, target_line, col)
}

fn line_start_bytes(s: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in s.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn col_in_line(s: &str, idx: usize, line_start: usize) -> usize {
    s[line_start..idx].chars().count()
}

fn byte_index_for_line_col(s: &str, line_starts: &[usize], line: usize, col: usize) -> usize {
    let start = *line_starts.get(line).unwrap_or(&0);
    let end = if line + 1 < line_starts.len() {
        // exclude newline
        line_starts[line + 1].saturating_sub(1)
    } else {
        s.len()
    };
    let slice = &s[start..end.min(s.len())];
    match slice.char_indices().nth(col) {
        Some((off, _)) => start + off,
        None => start + slice.len(),
    }
}

fn ensure_prompts_cursor_visible(model: &mut Model) {
    let w = model.prompts_viewport_width;
    let h = model.prompts_viewport_height;
    if w == 0 || h == 0 {
        return;
    }
    let cursor_row = wrapped_row_for_prefix(&model.prompts_raw, model.prompts_cursor, w);
    let scroll = model.prompts_scroll as usize;
    let view_h = h as usize;
    if cursor_row < scroll {
        model.prompts_scroll = cursor_row.min(u16::MAX as usize) as u16;
    } else if cursor_row >= scroll.saturating_add(view_h.saturating_sub(1)) {
        let new_scroll = cursor_row.saturating_sub(view_h.saturating_sub(1));
        model.prompts_scroll = new_scroll.min(u16::MAX as usize) as u16;
    }
}

fn wrapped_row_for_prefix(s: &str, cursor: usize, width: u16) -> usize {
    let cursor = cursor.min(s.len());
    let prefix = &s[..cursor];
    let wrapped = {
        // Same wrapping behavior as view.rs (preserve newlines).
        let mut out: Vec<String> = Vec::new();
        for line in prefix.lines() {
            out.extend(wrap_one_line(line, width));
        }
        if prefix.ends_with('\n') {
            out.push(String::new());
        }
        if out.is_empty() {
            out.push(String::new());
        }
        out
    };
    wrapped.len().saturating_sub(1)
}

fn wrap_one_line(s: &str, width: u16) -> Vec<String> {
    let w = width as usize;
    if w == 0 {
        return vec![String::new()];
    }
    if s.is_empty() {
        return vec![String::new()];
    }
    let opts = textwrap::Options::new(w).break_words(true);
    textwrap::wrap(s, &opts)
        .into_iter()
        .map(|c| c.into_owned())
        .collect()
}

fn update_modal_key(model: &mut Model, key: ratatui::crossterm::event::KeyEvent) -> Vec<Cmd> {
    let Some(modal) = model.modal.as_mut() else {
        return vec![];
    };

    match modal {
        Modal::MissingEnv { .. } | Modal::Info { .. } | Modal::Error { .. } => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                model.modal = None;
            }
            vec![]
        }
        Modal::CreateAgent(state) => {
            let (close, cmds) = update_create_agent_modal(state, key);
            if close {
                model.modal = None;
            }
            cmds
        }
    }
}

fn update_create_agent_modal(
    state: &mut CreateAgentModal,
    key: ratatui::crossterm::event::KeyEvent,
) -> (bool, Vec<Cmd>) {
    if key.code == KeyCode::Esc {
        return (true, vec![]);
    }

    if state.submitting {
        // While submitting, only allow cancel.
        return (false, vec![]);
    }

    // Tab switching between "Prompt" and "Tools" views.
    match key.code {
        KeyCode::Left => {
            state.tab = match state.tab {
                CreateAgentTab::Prompt => CreateAgentTab::Tools,
                CreateAgentTab::Tools => CreateAgentTab::Prompt,
            };
            return (false, vec![]);
        }
        KeyCode::Right => {
            state.tab = match state.tab {
                CreateAgentTab::Prompt => CreateAgentTab::Tools,
                CreateAgentTab::Tools => CreateAgentTab::Prompt,
            };
            return (false, vec![]);
        }
        _ => {}
    }

    // Tools tab interaction.
    if state.tab == CreateAgentTab::Tools {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if !state.tools.is_empty() {
                    state.tools_selected_index = state.tools_selected_index.saturating_sub(1);
                    state
                        .tools_list_state
                        .select(Some(state.tools_selected_index.min(state.tools.len() - 1)));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.tools.is_empty() {
                    state.tools_selected_index =
                        (state.tools_selected_index + 1).min(state.tools.len() - 1);
                    state.tools_list_state.select(Some(state.tools_selected_index));
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(t) = state.tools.get(state.tools_selected_index) {
                    toggle_tool(&t.id, &mut state.selected_tool_ids);
                }
            }
            KeyCode::Char('A') => {
                state.selected_tool_ids = state.tools.iter().map(|t| t.id.clone()).collect();
            }
            KeyCode::Char('X') => {
                state.selected_tool_ids.clear();
            }
            // submit still available from tools tab
            _ => {}
        }
    }

    // Submit (Ctrl+S).
    let submit = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s');
    if submit {
        state.error = None;
        let name = state.name.trim().to_string();
        let description = state.description.trim().to_string();
        let instructions = state.instructions.trim().to_string();

        if name.is_empty() {
            state.error = Some("Name is required.".to_string());
            state.focus = CreateAgentField::Name;
            return (false, vec![]);
        }
        if instructions.is_empty() {
            state.error = Some("Instructions/prompt is required.".to_string());
            state.focus = CreateAgentField::Instructions;
            return (false, vec![]);
        }
        if state.selected_tool_ids.is_empty() {
            state.error = Some("Select at least one tool (Tools tab).".to_string());
            state.tab = CreateAgentTab::Tools;
            return (false, vec![]);
        }

        let (is_edit, id) = match &state.mode {
            AgentEditorMode::Create => (false, generate_agent_id(&name)),
            AgentEditorMode::Edit { agent_id } => (true, agent_id.clone()),
        };
        let description = if description.is_empty() {
            name.clone()
        } else {
            description
        };

        state.submitting = true;
        return (
            false,
            vec![Cmd::UpsertAgent {
                is_edit,
                id,
                name,
                description,
                instructions,
                tool_ids: state.selected_tool_ids.clone(),
            }],
        );
    }

    // Prompt tab interaction (text entry).
    if state.tab != CreateAgentTab::Prompt {
        return (false, vec![]);
    }

    match key.code {
        KeyCode::Tab => {
            state.focus = match state.focus {
                CreateAgentField::Name => CreateAgentField::Description,
                CreateAgentField::Description => CreateAgentField::Instructions,
                CreateAgentField::Instructions => CreateAgentField::Name,
            };
        }
        KeyCode::BackTab => {
            state.focus = match state.focus {
                CreateAgentField::Name => CreateAgentField::Instructions,
                CreateAgentField::Description => CreateAgentField::Name,
                CreateAgentField::Instructions => CreateAgentField::Description,
            };
        }
        KeyCode::Enter => {
            if state.focus == CreateAgentField::Instructions {
                state.instructions.push('\n');
            } else {
                state.focus = match state.focus {
                    CreateAgentField::Name => CreateAgentField::Description,
                    CreateAgentField::Description => CreateAgentField::Instructions,
                    CreateAgentField::Instructions => CreateAgentField::Instructions,
                };
            }
        }
        KeyCode::Backspace => match state.focus {
            CreateAgentField::Name => {
                state.name.pop();
            }
            CreateAgentField::Description => {
                state.description.pop();
            }
            CreateAgentField::Instructions => {
                state.instructions.pop();
            }
        },
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return (false, vec![]);
            }
            match state.focus {
                CreateAgentField::Name => state.name.push(c),
                CreateAgentField::Description => state.description.push(c),
                CreateAgentField::Instructions => state.instructions.push(c),
            }
        }
        _ => {}
    }

    (false, vec![])
}

fn toggle_tool(id: &str, selected: &mut Vec<String>) {
    if let Some(pos) = selected.iter().position(|s| s == id) {
        selected.remove(pos);
    } else {
        selected.push(id.to_string());
        selected.sort();
    }
}

fn generate_agent_id(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("agent");
    }

    // Add a small suffix to reduce collisions.
    let suffix = chrono::Local::now().format("%H%M%S").to_string();
    format!("{out}-{suffix}")
}

#[derive(Debug, Clone, Copy)]
enum CycleDir {
    Forward,
    Backward,
}

fn cycle_panel(model: &mut Model, dir: CycleDir) {
    model.active = match (model.active, dir) {
        (ActivePanel::Top, CycleDir::Forward) => ActivePanel::Agents,
        (ActivePanel::Agents, CycleDir::Forward) => ActivePanel::Bottom,
        (ActivePanel::Bottom, CycleDir::Forward) => ActivePanel::Top,

        (ActivePanel::Top, CycleDir::Backward) => ActivePanel::Bottom,
        (ActivePanel::Bottom, CycleDir::Backward) => ActivePanel::Agents,
        (ActivePanel::Agents, CycleDir::Backward) => ActivePanel::Top,
    };
}

fn maybe_load_agents(model: &mut Model) -> Vec<Cmd> {
    if !model.config.is_ready() || !model.prompts_loaded {
        return vec![];
    }
    if model.agents_loaded || model.agents_loading {
        return vec![];
    }

    model.agents_loading = true;
    model.agents_error = None;
    model.active = ActivePanel::Agents;
    model.chat.push(super::model::ChatEntry::system(
        "Loading agents… (press ↑/↓ once loaded, Enter to select)",
    ));

    vec![Cmd::LoadAgents]
}

fn agent_prev(model: &mut Model) {
    if model.agents.is_empty() {
        return;
    }
    if model.agent_selected_index == 0 {
        model.agent_selected_index = model.agents.len() - 1;
    } else {
        model.agent_selected_index -= 1;
    }
    model.agents_list_state.select(Some(model.agent_selected_index));
}

fn agent_next(model: &mut Model) {
    if model.agents.is_empty() {
        return;
    }
    model.agent_selected_index = (model.agent_selected_index + 1) % model.agents.len();
    model.agents_list_state.select(Some(model.agent_selected_index));
}

#[derive(Debug, Clone, Copy)]
enum ScrollDir {
    Up,
    Down,
}

fn scroll_or_select(model: &mut Model, dir: ScrollDir, amount: u16) {
    if model.active == ActivePanel::Agents {
        // Keep agent selection predictable: wheel/arrow moves by 1, PageUp/Down jumps more.
        let steps = if amount >= 10 { 5 } else { 1 };
        for _ in 0..steps {
            match dir {
                ScrollDir::Up => agent_prev(model),
                ScrollDir::Down => agent_next(model),
            }
        }
        return;
    }

    match model.active {
        ActivePanel::Top => {
            model.prompts_scroll = scroll(model.prompts_scroll, dir, amount);
        }
        ActivePanel::Agents => {}
        ActivePanel::Bottom => match dir {
            ScrollDir::Up => {
                model.chat_scroll_from_bottom = model.chat_scroll_from_bottom.saturating_add(amount)
            }
            ScrollDir::Down => {
                model.chat_scroll_from_bottom = model.chat_scroll_from_bottom.saturating_sub(amount)
            }
        },
    }
}

fn scroll(current: u16, dir: ScrollDir, amount: u16) -> u16 {
    match dir {
        ScrollDir::Up => current.saturating_sub(amount),
        ScrollDir::Down => current.saturating_add(amount),
    }
}
