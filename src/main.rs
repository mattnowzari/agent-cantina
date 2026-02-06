mod app;
mod config;
mod elastic;
mod elm;
mod prompts;
mod theme;

fn main() -> anyhow::Result<()> {
    app::run()
}
