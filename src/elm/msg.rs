use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use super::model::ChatEntry;
use crate::config::Config;

#[derive(Debug, Clone)]
pub enum Msg {
    Init,
    Tick,
    Quit,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize,

    PromptsLoaded { raw: String, prompts: Vec<String> },
    PromptsLoadFailed { error: String },

    EnvLoaded { config: Config },

    AgentsLoaded { agents: Vec<crate::elastic::AgentSummary> },
    AgentsLoadFailed { error: String },

    AppendChat(ChatEntry),
    SetConversationId(Option<String>),
    RunStarted,
    RunFinished,
    RunFailed { error: String },

    SetWaiting(bool),
}
