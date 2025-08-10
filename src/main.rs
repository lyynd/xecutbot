use anyhow::Result;
use clap::Parser;
use xecut_bot::{Config, TelegramBot, Visits};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(short = 'c', long = "config", default_value = "xecut_bot")]
    config: Vec<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tokio::spawn(async move {
        pretty_env_logger::init_timed();
        let args = Cli::parse();

        let config = Config::new("xecut_bot", args.config)?;
        let visits = Visits::new(&config.db).await?;

        sqlx::migrate!("./migrations").run(visits.pool()).await?;

        let cancellation_token = visits.spawn_cleanup_task().await;

        let telegram_bot = TelegramBot::new(config.telegram_bot, visits).await?;

        telegram_bot.run().await;
        cancellation_token.cancel();
        Ok(())
    })
    .await?
}
