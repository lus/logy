mod probe;
mod receiver;

use anyhow::Result;
use clap::{Parser, Subcommand};
use probe::ProbeCommand;

use crate::cli::receiver::ReceiverCommand;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    color: colorchoice_clap::Color,

    #[command(subcommand)]
    command: Commands,

    /// Output plain JSON without color and interactivity.
    #[arg(short, long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    Probe(ProbeCommand),
    Receiver(ReceiverCommand),
}

pub async fn execute() -> Result<()> {
    let cli = Cli::parse();

    cli.color.write_global();

    match &cli.command {
        Commands::Probe(cmd) => cmd.execute(&cli).await,
        Commands::Receiver(cmd) => cmd.execute(&cli).await,
    }
}
