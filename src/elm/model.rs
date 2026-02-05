use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Model {
    pub should_quit: bool,
    pub active: ActivePanel,

    pub prompts_path: String,
    pub prompts_raw: String,
    pub prompts: Vec<String>,
    pub prompts_scroll: u16,

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

            prompts_path: "PROMPTS.md".to_string(),
            prompts_raw: String::new(),
            prompts: Vec::new(),
            prompts_scroll: 0,

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
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    Done,
    Error,
}

impl Default for RunState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub enum Modal {
    MissingEnv { missing: Vec<&'static str> },
    Info { title: String, message: String },
    Error { title: String, message: String },
}

#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role: ChatRole,
    pub content: String,
}

impl ChatEntry {
    pub fn system(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: msg.into(),
        }
    }

    pub fn user(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: msg.into(),
        }
    }

    pub fn agent(msg: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Agent,
            content: msg.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Agent,
}
