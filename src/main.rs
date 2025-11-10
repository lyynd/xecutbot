use anyhow::Result;
use clap::Parser;
use xecut_bot::backend::BackendImpl;

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
        let backend = BackendImpl::new(args.config).await?;
        backend.run().await?;
        Ok(())
    })
    .await?
}
