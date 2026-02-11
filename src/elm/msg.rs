use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use super::model::ChatEntry;
use crate::config::Config;
use crate::elastic::AgentSummary;
use crate::elastic::ToolSummary;

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
    PromptsSaved { raw: String, prompts: Vec<String> },
    PromptsSaveFailed { error: String },

    ConversationDumped { path: String },
    ConversationDumpFailed { error: String },

    EnvLoaded { config: Config },

    AgentsLoaded { agents: Vec<crate::elastic::AgentSummary> },
    AgentsLoadFailed { error: String },

    ToolsLoaded { tools: Vec<ToolSummary> },
    ToolsLoadFailed { error: String },

    AgentCreated { agent: AgentSummary },
    AgentCreateFailed { error: String },

    AppendChat(ChatEntry),
    SetConversationId(Option<String>),
    RunStarted,
    RunFinished,
    RunFailed { error: String },

    SetWaiting(bool),
}
