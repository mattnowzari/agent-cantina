use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::elm::{Cmd, Model, Msg};

pub fn run() -> Result<()> {
    // When running a TUI in raw mode, panic output can be invisible (or leave the
    // terminal in a bad state). Restore the terminal and print the panic + backtrace.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableMouseCapture
        );
        ratatui::restore();

        let bt = std::backtrace::Backtrace::capture();
        eprintln!("\n\nAgent Cantina panicked: {info}\n\nBacktrace:\n{bt}\n");
        prev_hook(info);
    }));

    let terminal = ratatui::init();
    // Ensure mouse-wheel events (scrolling) work.
    ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::EnableMouseCapture
    )?;

    let result = run_with_terminal(terminal);

    // Best-effort cleanup (restore() also does terminal cleanup, but we explicitly
    // disable mouse capture so terminals that support it behave consistently).
    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::DisableMouseCapture
    );
    ratatui::restore();
    result
}

fn run_with_terminal(mut terminal: DefaultTerminal) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Msg>();

    let mut model = Model::default();
    let mut queue: std::collections::VecDeque<Msg> = std::collections::VecDeque::new();

    // Kick off initial load.
    queue.push_back(Msg::Init);

    while !model.should_quit {
        // Drain any background messages.
        while let Ok(msg) = rx.try_recv() {
            queue.push_back(msg);
        }

        // Read user input (or tick).
        if let Some(msg) = read_msg()? {
            queue.push_back(msg);
        }

        // Process message queue.
        while let Some(msg) = queue.pop_front() {
            let cmds = crate::elm::update(&mut model, msg);
            for cmd in cmds {
                execute_cmd(&rt, tx.clone(), &model, cmd);
            }
        }

        terminal.draw(|frame| crate::elm::view(frame, &mut model))?;
    }

    Ok(())
}

fn read_msg() -> Result<Option<Msg>> {
    use ratatui::crossterm::event::{self, Event, KeyCode};
    use std::time::Duration;

    if !event::poll(Duration::from_millis(250))? {
        return Ok(Some(Msg::Tick));
    }

    match event::read()? {
        Event::Key(key) => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL)
            {
                Ok(Some(Msg::Quit))
            } else {
                Ok(Some(Msg::Key(key)))
            }
        }
        Event::Mouse(mouse) => Ok(Some(Msg::Mouse(mouse))),
        Event::Resize(_, _) => Ok(Some(Msg::Resize)),
        _ => Ok(None),
    }
}

fn execute_cmd(
    rt: &tokio::runtime::Runtime,
    tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    model: &Model,
    cmd: Cmd,
) {
    match cmd {
        Cmd::LoadPromptsFile { path } => {
            rt.spawn(async move {
                let res = tokio::task::spawn_blocking(move || {
                    crate::prompts::load_or_create_prompts_file(&path)
                })
                .await;

                match res {
                    Ok(Ok(pf)) => {
                        let _ = tx.send(Msg::PromptsLoaded {
                            raw: pf.raw,
                            prompts: pf.prompts,
                        });
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(Msg::PromptsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::PromptsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
        Cmd::SavePromptsFile { path, raw } => {
            rt.spawn(async move {
                let res = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::prompts::PromptFile> {
                    std::fs::write(&path, &raw)?;
                    Ok(crate::prompts::parse_prompts_markdown(raw))
                })
                .await;

                match res {
                    Ok(Ok(pf)) => {
                        let _ = tx.send(Msg::PromptsSaved {
                            raw: pf.raw,
                            prompts: pf.prompts,
                        });
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(Msg::PromptsSaveFailed {
                            error: e.to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::PromptsSaveFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
        Cmd::DumpConversationMarkdown { path, markdown } => {
            rt.spawn(async move {
                let path_for_send = path.clone();
                let res = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                    std::fs::write(&path, markdown)?;
                    Ok(())
                })
                .await;

                match res {
                    Ok(Ok(())) => {
                        let _ = tx.send(Msg::ConversationDumped { path: path_for_send });
                    }
                    Ok(Err(e)) => {
                        let _ = tx.send(Msg::ConversationDumpFailed {
                            error: e.to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ConversationDumpFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
        Cmd::LoadEnv => {
            rt.spawn(async move {
                let cfg = tokio::task::spawn_blocking(crate::config::load_from_env)
                    .await
                    .unwrap_or_default();
                let _ = tx.send(Msg::EnvLoaded { config: cfg });
            });
        }
        Cmd::LoadAgents => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentsLoadFailed {
                        error: "Missing KIBANA_URL/ES_HOST and/or API_KEY/ES_API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::elastic::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };

                match client.list_agents().await {
                    Ok(agents) => {
                        let _ = tx.send(Msg::AgentsLoaded { agents });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::AgentsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
        Cmd::LoadTools => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::ToolsLoadFailed {
                        error: "Missing KIBANA_URL/ES_HOST and/or API_KEY/ES_API_KEY.".to_string(),
                    });
                    return;
                }
                let client = match crate::elastic::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::ToolsLoadFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };

                match client.list_tools().await {
                    Ok(tools) => {
                        let _ = tx.send(Msg::ToolsLoaded { tools });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::ToolsLoadFailed {
                            error: e.to_string(),
                        });
                    }
                }
            });
        }
        Cmd::StartRun => {
            // Snapshot required state for the background run.
            let cfg = model.config.clone();
            let prompts = model.prompts.clone();
            let selected_agent_id = model.selected_agent_id.clone();

            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::RunFailed {
                        error: "Missing KIBANA_URL (or ES_HOST) and/or API_KEY (or ES_API_KEY)."
                            .to_string(),
                    });
                    return;
                }
                if selected_agent_id.as_deref().unwrap_or("").is_empty() {
                    let _ = tx.send(Msg::RunFailed {
                        error: "No agent selected. Pick an agent first, then run.".to_string(),
                    });
                    return;
                }
                if prompts.is_empty() {
                    let _ = tx.send(Msg::RunFailed {
                        error: "No prompts parsed from PROMPTS.md.".to_string(),
                    });
                    return;
                }

                let _ = tx.send(Msg::SetConversationId(None));
                let _ = tx.send(Msg::RunStarted);

                let mut cfg = cfg;
                if let Some(agent_id) = selected_agent_id {
                    cfg.agent_id = agent_id;
                }
                let client = match crate::elastic::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::RunFailed {
                            error: e.to_string(),
                        });
                        return;
                    }
                };

                let mut conversation_id: Option<String> = None;
                for prompt in prompts {
                    let _ = tx.send(Msg::AppendChat(crate::elm::ChatEntry::user(prompt.clone())));

                    let _ = tx.send(Msg::SetWaiting(true));
                    match client.converse(&prompt, conversation_id.as_deref()).await {
                        Ok(res) => {
                            let _ = tx.send(Msg::SetWaiting(false));
                            conversation_id = res.conversation_id.or(conversation_id);
                            let _ = tx.send(Msg::SetConversationId(conversation_id.clone()));
                            let _ =
                                tx.send(Msg::AppendChat(crate::elm::ChatEntry::agent(res.message)));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::SetWaiting(false));
                            let _ = tx.send(Msg::RunFailed {
                                error: e.to_string(),
                            });
                            return;
                        }
                    }
                }

                let _ = tx.send(Msg::RunFinished);
            });
        }
        Cmd::UpsertAgent {
            is_edit,
            id,
            name,
            description,
            instructions,
            tool_ids,
        } => {
            let cfg = model.config.clone();
            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::AgentUpsertFailed {
                        error: "Missing KIBANA_URL/ES_HOST and/or API_KEY/ES_API_KEY.".to_string(),
                        is_edit,
                    });
                    return;
                }
                let client = match crate::elastic::AgentBuilderClient::new(&cfg) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: e.to_string(),
                            is_edit,
                        });
                        return;
                    }
                };

                let config = crate::elastic::CreateAgentConfiguration {
                    instructions: Some(instructions),
                    tools: vec![crate::elastic::CreateAgentTools { tool_ids }],
                };

                let res = if is_edit {
                    client
                        .update_agent(
                            &id,
                            crate::elastic::UpdateAgentRequest {
                                name,
                                description,
                                configuration: config,
                                avatar_color: None,
                                avatar_symbol: None,
                                labels: vec![],
                            },
                        )
                        .await
                } else {
                    client
                        .create_agent(crate::elastic::CreateAgentRequest {
                            id,
                            name,
                            description,
                            configuration: config,
                            // Keep avatar minimal; users can edit later in Kibana.
                            avatar_color: None,
                            avatar_symbol: None,
                            labels: vec![],
                        })
                        .await
                };

                match res {
                    Ok(agent) => {
                        let _ = tx.send(Msg::AgentUpserted { agent, is_edit });
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::AgentUpsertFailed {
                            error: e.to_string(),
                            is_edit,
                        });
                    }
                };
            });
        }
    }
}
