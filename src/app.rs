use anyhow::Result;
use ratatui::DefaultTerminal;

use crate::elm::{Cmd, Model, Msg};

pub fn run() -> Result<()> {
    let terminal = ratatui::init();
    let result = run_with_terminal(terminal);
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

        terminal.draw(|frame| crate::elm::view(frame, &model))?;
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
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q')) {
                Ok(Some(Msg::Quit))
            } else if key.code == KeyCode::Char('c')
                && key.modifiers.contains(event::KeyModifiers::CONTROL)
            {
                Ok(Some(Msg::Quit))
            } else {
                Ok(Some(Msg::Key(key)))
            }
        }
        Event::Resize(w, h) => Ok(Some(Msg::Resize { w, h })),
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
        Cmd::LoadEnv => {
            rt.spawn(async move {
                let cfg = tokio::task::spawn_blocking(crate::config::load_from_env)
                    .await
                    .unwrap_or_default();
                let _ = tx.send(Msg::EnvLoaded { config: cfg });
            });
        }
        Cmd::StartRun => {
            // Snapshot required state for the background run.
            let cfg = model.config.clone();
            let prompts = model.prompts.clone();

            rt.spawn(async move {
                if !cfg.is_ready() {
                    let _ = tx.send(Msg::RunFailed {
                        error: "Missing KIBANA_URL (or ES_HOST) and/or API_KEY (or ES_API_KEY)."
                            .to_string(),
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
    }
}
