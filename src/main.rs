use anyhow::Result;
use clap::Parser;
use xecut_bot::{Config, Handler, Visits};

#[derive(Parser)]
struct Cli {
    #[arg(short = 'c', long = "config", default_value = "xecut_bot")]
    config: std::path::PathBuf,
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

        let handler = Handler::new(config.telegram_bot, visits).await?;

        handler.run().await;
        cancellation_token.cancel();
        Ok(())
    })
    .await?
}
