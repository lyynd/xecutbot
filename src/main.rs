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

        let cleanup_ct = visits.spawn_cleanup_task().await;

        let telegram_bot = TelegramBot::new(config.telegram_bot, visits).await?;

        let live_update_ct = telegram_bot.spawn_update_live_task().await;

        telegram_bot.run().await;

        live_update_ct.cancel();
        cleanup_ct.cancel();

        Ok(())
    })
    .await?
}
