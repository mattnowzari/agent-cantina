#[derive(Debug, Clone)]
pub enum Cmd {
    LoadPromptsFile { path: String },
    SavePromptsFile { path: String, raw: String },
    DumpConversationMarkdown { path: String, markdown: String },
    IndexConversationToEs { index: String, id: String, doc: serde_json::Value },
    LoadEnv,
    LoadAgents,
    LoadTools,
    StartRun,
    UpsertAgent {
        is_edit: bool,
        id: String,
        name: String,
        description: String,
        instructions: String,
        tool_ids: Vec<String>,
    },
}
