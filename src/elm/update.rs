use ratatui::crossterm::event::KeyCode;

use super::{Cmd, Model, Msg, model::ActivePanel};

pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    // When a modal is open, only allow dismiss + quit.
    if model.modal.is_some() {
        match msg {
            Msg::Quit => model.should_quit = true,
            Msg::Key(key) => match key.code {
                KeyCode::Enter | KeyCode::Esc => model.modal = None,
                _ => {}
            },
            Msg::DismissModal => model.modal = None,
            _ => {}
        }
        return vec![];
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

                // Scrolling
                KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => {
                    if model.active == ActivePanel::Agents {
                        agent_prev(model);
                    } else {
                        scroll_active(model, ScrollDir::Up);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => {
                    if model.active == ActivePanel::Agents {
                        agent_next(model);
                    } else {
                        scroll_active(model, ScrollDir::Down);
                    }
                }
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
                    return maybe_load_agents(model);
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

        Msg::Tick => {
            if model.waiting_for_response {
                model.spinner_frame = model.spinner_frame.wrapping_add(1);
            }
            vec![]
        }
        Msg::Resize { .. } => vec![],

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

        Msg::AgentSelected { agent_id } => {
            model.selected_agent_id = Some(agent_id.clone());
            model.config.agent_id = agent_id;
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

        Msg::DismissModal => {
            model.modal = None;
            vec![]
        }
    }
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
}

fn agent_next(model: &mut Model) {
    if model.agents.is_empty() {
        return;
    }
    model.agent_selected_index = (model.agent_selected_index + 1) % model.agents.len();
}

#[derive(Debug, Clone, Copy)]
enum ScrollDir {
    Up,
    Down,
}

fn scroll_active(model: &mut Model, dir: ScrollDir) {
    let amount: u16 = 1;
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
