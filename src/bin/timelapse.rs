#[path = "../cli.rs"]
mod cli;

fn main() -> anyhow::Result<()> {
    cli::run()
}
