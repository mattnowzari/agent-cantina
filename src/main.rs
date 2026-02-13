mod app;
mod config;
mod es;
mod agentbuilder;
mod elm;
mod prompts;
mod theme;

fn main() -> anyhow::Result<()> {
    app::run()
}
