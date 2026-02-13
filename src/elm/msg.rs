use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use super::model::ChatEntry;
use crate::config::Config;
use crate::agentbuilder::AgentSummary;
use crate::agentbuilder::ToolSummary;

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
    ConversationIndexed { index: String, id: String },
    ConversationIndexFailed { error: String },

    EnvLoaded { config: Config },

    AgentsLoaded { agents: Vec<crate::agentbuilder::AgentSummary> },
    AgentsLoadFailed { error: String },

    ToolsLoaded { tools: Vec<ToolSummary> },
    ToolsLoadFailed { error: String },

    AgentUpserted { agent: AgentSummary, is_edit: bool },
    AgentUpsertFailed { error: String, is_edit: bool },

    AppendChat(ChatEntry),
    SetConversationId(Option<String>),
    RunStarted,
    RunFinished,
    RunFailed { error: String },

    SetWaiting(bool),
}
