use anyhow::Result;

use crate::config::Config;

mod bot;
mod config;
mod utils;
mod visits;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init_timed();

    let config = Config::new()?;

    tokio::spawn({
        let handler = bot::Handler::new(config).await?;
        async move {
            handler.run().await;
        }
    })
    .await?;

    Ok(())
}
