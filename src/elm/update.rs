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
                KeyCode::Up | KeyCode::Left => model.active = ActivePanel::Top,
                KeyCode::Down | KeyCode::Right => model.active = ActivePanel::Bottom,
                KeyCode::Tab => {
                    model.active = match model.active {
                        ActivePanel::Top => ActivePanel::Bottom,
                        ActivePanel::Bottom => ActivePanel::Top,
                    }
                }

                // Scrolling
                KeyCode::Char('k') | KeyCode::PageUp => scroll_active(model, ScrollDir::Up),
                KeyCode::Char('j') | KeyCode::PageDown => scroll_active(model, ScrollDir::Down),
                KeyCode::End => {
                    if model.active == ActivePanel::Bottom {
                        model.chat_scroll_from_bottom = 0;
                    }
                }

                // Re-run prompts
                KeyCode::Char('r') => {
                    if model.run_state != super::model::RunState::Running {
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

            // Auto-run if configured.
            if model.config.is_ready()
                && !model.prompts.is_empty()
                && model.run_state == super::model::RunState::Idle
            {
                model.run_state = super::model::RunState::Running;
                vec![Cmd::StartRun]
            } else {
                vec![]
            }
        }

        Msg::PromptsLoadFailed { error } => {
            model.modal = Some(super::model::Modal::Error {
                title: "Failed to load PROMPTS.md".to_string(),
                message: error,
            });
            vec![]
        }

        Msg::EnvLoaded { config } => {
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

            if !model.prompts.is_empty() && model.run_state == super::model::RunState::Idle {
                model.run_state = super::model::RunState::Running;
                vec![Cmd::StartRun]
            } else {
                vec![]
            }
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
