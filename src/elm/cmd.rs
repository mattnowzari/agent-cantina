#[derive(Debug, Clone)]
pub enum Cmd {
    LoadPromptsFile { path: String },
    LoadEnv,
    StartRun,
}
