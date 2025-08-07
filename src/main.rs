use anyhow::Result;
use xecut_bot::{Config, Handler, Visits};

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init_timed();
    let config = Config::new("xecut_bot")?;

    tokio::spawn({
        let visits = Visits::new(&config.db)
            .await
            .expect("DB initialization success");
        let handler = Handler::new(config.telegram_bot, visits).await?;
        async move {
            handler.run().await;
        }
    })
    .await?;

    Ok(())
}
