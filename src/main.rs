mod app;
mod config;
mod elastic;
mod elm;
mod prompts;

fn main() -> anyhow::Result<()> {
    app::run()
}
