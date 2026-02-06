use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use super::{
    Cmd, Model, Msg,
    model::{ActivePanel, CreateAgentField, CreateAgentModal, Modal},
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

                // Refresh agent list
                KeyCode::Char('g') => {
                    model.agents_loaded = false;
                    model.agents_loading = false;
                    model.agents_error = None;
                    model.agents.clear();
                    model.agent_selected_index = 0;
                    model.selected_agent_id = None;
                    model.agents_list_state = ratatui::widgets::ListState::default();
                    return maybe_load_agents(model);
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
                        model.modal = Some(Modal::CreateAgent(CreateAgentModal::default()));
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
            if model.waiting_for_response {
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
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to load agents".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::AgentCreated { agent } => {
            model.chat.push(super::model::ChatEntry::system(format!(
                "Created agent: {} ({})",
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

        Msg::AgentCreateFailed { error } => {
            model.chat.push(super::model::ChatEntry::system(format!(
                "Create agent failed: {}",
                error
            )));
            // If the create-agent modal is open, show the error inline there.
            if let Some(Modal::CreateAgent(state)) = model.modal.as_mut() {
                state.submitting = false;
                state.error = Some(error);
            } else {
                model.modal = Some(Modal::Error {
                    title: "Failed to create agent".to_string(),
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

    // Submit (avoid relying only on Ctrl+S, which can be flow-control in some terminals).
    let submit = (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s'))
        || key.code == KeyCode::F(2);
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

        let id = generate_agent_id(&name);
        let description = if description.is_empty() {
            format!("Custom agent created in Agent Cantina: {name}")
        } else {
            description
        };

        state.submitting = true;
        return (
            false,
            vec![Cmd::CreateAgent {
                id,
                name,
                description,
                instructions,
            }],
        );
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
