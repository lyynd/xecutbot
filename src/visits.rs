use std::time::Duration;

use crate::{Config, bot::Uid, utils::today};
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use sqlx::sqlite::SqlitePool;
use tokio_util::sync::{CancellationToken, DropGuard};

#[derive(Debug, Clone)]
pub struct Visit {
    pub person: Uid,
    pub day: NaiveDate,
    pub purpose: String,
}

pub struct Visits {
    pool: SqlitePool,
    #[allow(dead_code)]
    cleanup_cancellation_drop_token: DropGuard,
}

const VISIT_HISTORY_DAYS: i32 = 30;
const VISITS_CLEANUP_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

impl Visits {
    pub async fn new(config: &Config) -> Result<Visits, anyhow::Error> {
        let pool = SqlitePool::connect(&config.sqlite_path).await?;

        let pool_clone = pool.clone();

        let cancellation_token = CancellationToken::new();
        let cancellation_token_listen = cancellation_token.clone();

        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(VISITS_CLEANUP_INTERVAL);

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = cancellation_token_listen.cancelled() => { break }
                };
                Self::cleanup(&pool_clone)
                    .await
                    .expect("successful cleanup");
            }
        });

        Ok(Visits {
            pool,
            cleanup_cancellation_drop_token: cancellation_token.drop_guard(),
        })
    }

    pub async fn get_visits(&self) -> Result<Vec<Visit>> {
        let current_day = today().num_days_from_ce();
        Ok(sqlx::query!(
            "SELECT person, day, purpose FROM visit WHERE day >= ?1",
            current_day
        )
        .map(|r| Visit {
            person: r.person.into(),
            day: NaiveDate::from_num_days_from_ce_opt(r.day.try_into().unwrap()).unwrap(),
            purpose: r.purpose,
        })
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn add_visit(&self, visit: Visit) -> Result<bool> {
        let person: i64 = visit.person.into();
        let day = visit.day.num_days_from_ce();

        let mut tx = self.pool.begin().await?;

        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM visit WHERE person = ?1 AND day = ?2) AS "exists: bool""#,
            person,
            day
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
                "INSERT INTO visit (person, day, purpose) VALUES (?1, ?2, ?3) ON CONFLICT DO UPDATE SET purpose = excluded.purpose",
                person,
                day,
                visit.purpose
            )
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(!exists)
    }

    pub async fn delete_visit(&self, person: Uid, day: NaiveDate) -> Result<bool> {
        let person: i64 = person.into();
        let day = day.num_days_from_ce();
        Ok(sqlx::query!(
            "DELETE FROM visit WHERE person = ?1 AND day = ?2",
            person,
            day
        )
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn cleanup(pool: &SqlitePool) -> Result<()> {
        let current_day = Local::now().date_naive().num_days_from_ce();
        let cutoff = current_day - VISIT_HISTORY_DAYS;

        sqlx::query!("DELETE FROM visit WHERE day < ?1", cutoff)
            .execute(pool)
            .await?;

        Ok(())
    }
}
