use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "retake",
    about = "Local-first review tool for AI-generated work"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start as MCP server (stdio)
    Mcp,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("retake=info")
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Mcp => {
            retake_server::run_mcp().await?;
        }
    }
    Ok(())
}
