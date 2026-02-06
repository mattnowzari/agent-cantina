#[derive(Debug, Clone)]
pub enum Cmd {
    LoadPromptsFile { path: String },
    LoadEnv,
    LoadAgents,
    StartRun,
    CreateAgent {
        id: String,
        name: String,
        description: String,
        instructions: String,
    },
}
