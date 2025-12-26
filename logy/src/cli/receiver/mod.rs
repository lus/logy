mod info;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use hidpp::receiver::{self, Receiver};

use crate::{
    async_hid_impl::enumerate_hidpp,
    cli::{Cli, receiver::info::ReceiverInfoCommand},
};

/// View information about receivers and pair/unpair devices to/from them.
#[derive(Args)]
pub struct ReceiverCommand {
    #[command(subcommand)]
    command: Commands,

    /// The unique ID of the receiver.
    #[arg(short, long, global = true)]
    receiver_id: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    Info(ReceiverInfoCommand),
}

impl ReceiverCommand {
    pub async fn execute(&self, root: &Cli) -> Result<()> {
        let mut receivers: Vec<Receiver> = enumerate_hidpp()
            .await?
            .into_iter()
            .filter_map(|x| receiver::detect(Arc::new(x)))
            .collect();

        let mut selected_receiver = None;
        if let Some(id) = &self.receiver_id {
            for receiver in receivers {
                let receiver_id = receiver.get_unique_id().await?;
                if receiver_id == *id {
                    selected_receiver.replace(receiver);
                    break;
                }
            }
        } else if receivers.len() == 1 {
            selected_receiver.replace(receivers.pop().unwrap());
        }

        if selected_receiver.is_none() {
            return Err(anyhow!("no receiver could be found"));
        }

        match &self.command {
            Commands::Info(cmd) => cmd.execute(root).await,
        }
    }
}
