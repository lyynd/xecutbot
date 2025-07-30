use crate::config::Config;

mod bot;
mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init_timed();

    let config = Config::new()?;

    tokio::spawn({
        let handler = bot::Handler::new(config);
        async move {
            handler.run().await;
        }
    })
    .await?;

    Ok(())
}
