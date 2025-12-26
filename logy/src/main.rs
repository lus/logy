use anyhow::Result;

mod async_hid_impl;
mod cli;
mod hidpp_ext;

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute().await
}
