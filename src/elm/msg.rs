use ratatui::crossterm::event::KeyEvent;

use super::model::ChatEntry;
use crate::config::Config;

#[derive(Debug, Clone)]
pub enum Msg {
    Init,
    Tick,
    Quit,
    Key(KeyEvent),
    Resize { w: u16, h: u16 },

    PromptsLoaded { raw: String, prompts: Vec<String> },
    PromptsLoadFailed { error: String },

    EnvLoaded { config: Config },

    AppendChat(ChatEntry),
    SetConversationId(Option<String>),
    RunStarted,
    RunFinished,
    RunFailed { error: String },

    SetWaiting(bool),

    DismissModal,
}
