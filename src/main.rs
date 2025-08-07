use anyhow::Result;
use xecut_bot::{Config, Handler};

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init_timed();
    let config = Config::new()?;

    tokio::spawn({
        let handler = Handler::new(config).await?;
        async move {
            handler.run().await;
        }
    })
    .await?;

    Ok(())
}
