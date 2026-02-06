#[derive(Debug, Clone)]
pub enum Cmd {
    LoadPromptsFile { path: String },
    LoadEnv,
    LoadAgents,
    LoadTools,
    StartRun,
    CreateAgent {
        id: String,
        name: String,
        description: String,
        instructions: String,
        tool_ids: Vec<String>,
    },
}
