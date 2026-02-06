use crate::config::Config;
use crate::elastic::AgentSummary;
use ratatui::widgets::ListState;

#[derive(Debug)]
pub struct Model {
    pub should_quit: bool,
    pub active: ActivePanel,

    pub prompts_loaded: bool,
    pub env_loaded: bool,

    pub prompts_path: String,
    pub prompts_raw: String,
    pub prompts: Vec<String>,
    pub prompts_scroll: u16,

    pub agents_loading: bool,
    pub agents_loaded: bool,
    pub agents_error: Option<String>,
    pub agents: Vec<AgentSummary>,
    pub agent_selected_index: usize,
    pub selected_agent_id: Option<String>,
    pub agents_list_state: ListState,

    pub chat: Vec<ChatEntry>,
    /// How many lines above the bottom we are scrolled.
    /// `0` means "follow the latest messages".
    pub chat_scroll_from_bottom: u16,

    pub config: Config,
    pub run_state: RunState,
    pub conversation_id: Option<String>,

    pub waiting_for_response: bool,
    pub spinner_frame: usize,

    pub modal: Option<Modal>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            should_quit: false,
            active: ActivePanel::Top,

            prompts_loaded: false,
            env_loaded: false,

            prompts_path: "PROMPTS.md".to_string(),
            prompts_raw: String::new(),
            prompts: Vec::new(),
            prompts_scroll: 0,

            agents_loading: false,
            agents_loaded: false,
            agents_error: None,
            agents: Vec::new(),
            agent_selected_index: 0,
            selected_agent_id: None,
            agents_list_state: ListState::default(),

            chat: vec![ChatEntry::system(
                "Loading PROMPTS.md and checking env (KIBANA_URL/ES_HOST, API_KEY/ES_API_KEY)…",
            )],
            chat_scroll_from_bottom: 0,

            config: Config::default(),
            run_state: RunState::Idle,
            conversation_id: None,

            waiting_for_response: false,
            spinner_frame: 0,

            modal: None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    #[default]
    Top,
    Agents,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum RunState {
    #[default]
    Idle,
    Running,
    Done,
    Error,
}


#[derive(Debug, Clone)]
pub enum Modal {
    MissingEnv { missing: Vec<&'static str> },
    Info { title: String, message: String },
    Error { title: String, message: String },
    CreateAgent(CreateAgentModal),
}

#[derive(Debug, Clone)]
pub struct CreateAgentModal {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub focus: CreateAgentField,
    pub submitting: bool,
    pub error: Option<String>,
}

impl Default for CreateAgentModal {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            instructions: String::new(),
            focus: CreateAgentField::Name,
            submitting: false,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateAgentField {
    Name,
    Description,
    Instructions,
}

#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role: ChatRole,
    pub timestamp: String,
    pub content: String,
}

impl ChatEntry {
    pub fn system(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            timestamp: now_timestamp(),
            content: msg.into(),
        }
    }

    pub fn user(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            timestamp: now_timestamp(),
            content: msg.into(),
        }
    }

    pub fn agent(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Agent,
            timestamp: now_timestamp(),
            content: msg.into(),
        }
    }
}

fn now_timestamp() -> String {
    // Keep this cheap + stable for TUI display.
    chrono::Local::now().format("%H:%M:%S").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Agent,
}
