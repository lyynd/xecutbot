use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use sqlx::SqlitePool;

use crate::config::DbConfig;
use crate::rest_api::RestApi;
use crate::{Config, TelegramBot, Visits};

#[derive(Clone)]
pub struct BackendImpl {
    pub pool: SqlitePool,
    pub visits: Visits,
    pub tg_bot: Arc<TelegramBot<Self>>,
    pub rest_api: RestApi<Self>,
}

pub trait Backend: Sized {
    fn pool(&self) -> &SqlitePool;
    fn visits(&self) -> &Visits;
    fn tg_bot(&self) -> &TelegramBot<Self>;
}

impl Backend for BackendImpl {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn visits(&self) -> &Visits {
        &self.visits
    }

    fn tg_bot(&self) -> &TelegramBot<Self> {
        &self.tg_bot
    }
}

pub async fn connect_db(db_config: &DbConfig) -> Result<SqlitePool> {
    Ok(SqlitePool::connect(&db_config.sqlite_path).await?)
}

impl BackendImpl {
    pub async fn new(config_files: Vec<PathBuf>) -> Result<Arc<Self>> {
        let config = Config::new("xecut_bot", config_files)?;

        let pool = connect_db(&config.db).await?;

        let visits = Visits::new(pool.clone())?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        let backend = Arc::new_cyclic(|backend| BackendImpl {
            pool,
            visits,
            tg_bot: TelegramBot::new(config.telegram_bot, backend.clone()).unwrap(),
            rest_api: RestApi::new(config.rest_api, backend.clone()),
        });

        Ok(backend)
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        let results = tokio::try_join!(
            tokio::spawn(self.visits.clone().run()),
            tokio::spawn(self.tg_bot.clone().run()),
            tokio::spawn(self.rest_api.clone().run())
        )?;
        results.1?;
        results.2?;

        Ok(())
    }
}
