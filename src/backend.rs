use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use teloxide::types::UserId;

use crate::config::DbConfig;
use crate::rest_api::RestApi;
use crate::utils::today;
use crate::visits::VisitUpdate;
use crate::{Config, TelegramBot, VisitStatus, Visits};

#[derive(Clone)]
pub struct BackendImpl {
    pub pool: SqlitePool,
    pub visits: Visits,
    pub tg_bot: Arc<TelegramBot<Self>>,
    pub rest_api: RestApi<Self>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uid(pub UserId);

impl From<i64> for Uid {
    fn from(value: i64) -> Self {
        Uid(UserId(value as u64))
    }
}

impl From<Uid> for i64 {
    fn from(val: Uid) -> Self {
        val.0.0 as i64
    }
}

pub trait Backend: Sized + Send + Sync + 'static {
    fn pool(&self) -> &SqlitePool;
    fn visits(&self) -> &Visits;
    fn tg_bot(&self) -> &TelegramBot<Self>;

    fn check_in(
        &self,
        person: Uid,
        purpose: Option<String>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let visit_update = VisitUpdate {
                person,
                day: today(),
                purpose,
                status: VisitStatus::CheckedIn,
            };

            let updated = self.visits().upsert_visit(&visit_update).await?;

            if updated {
                self.tg_bot().announce_check_in(&visit_update).await?;
            }

            Ok(())
        }
    }

    fn check_out(&self, person: Uid) -> impl Future<Output = Result<()>> + Send {
        async move {
            let visit_update = VisitUpdate {
                person,
                day: today(),
                purpose: None,
                status: VisitStatus::CheckedOut,
            };

            self.visits().upsert_visit(&visit_update).await?;

            Ok(())
        }
    }

    fn plan_visit(
        &self,
        person: Uid,
        day: NaiveDate,
        purpose: Option<String>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let visit_update = VisitUpdate {
                person,
                day,
                purpose,
                status: VisitStatus::Planned,
            };

            let updated = self.visits().upsert_visit(&visit_update).await?;

            if updated {
                self.tg_bot().announce_plan(&visit_update).await?;
            }

            maybe_panic(visit_update.purpose.as_deref().unwrap_or_default())?;

            Ok(())
        }
    }

    fn unplan_visit(&self, person: Uid, day: NaiveDate) -> impl Future<Output = Result<()>> + Send {
        async move {
            let deleted = self.visits().delete_visit(person, day).await?;

            if deleted {
                self.tg_bot().announce_unplan(person, day).await?;
            }

            Ok(())
        }
    }
}

fn maybe_panic(text: &str) -> Result<()> {
    match text {
        "panic" => panic!("ayaya"),
        "error" => anyhow::bail!("ayayaya"),
        _ => Ok(()),
    }
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
