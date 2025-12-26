use anyhow::Result;
use clap::Args;

use crate::cli::Cli;

/// View information about a receiver.
#[derive(Args)]
pub struct ReceiverInfoCommand {}

impl ReceiverInfoCommand {
    pub async fn execute(&self, root: &Cli) -> Result<()> {
        Ok(())
    }
}
