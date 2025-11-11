use std::time::Duration;

use crate::backend::Uid;
use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use sqlx::sqlite::SqlitePool;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitStatus {
    Planned,
    CheckedIn,
    CheckedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub person: Uid,
    pub day: NaiveDate,
    pub purpose: String,
    pub status: VisitStatus,
}

#[derive(Debug, Clone)]
pub struct VisitUpdate {
    pub person: Uid,
    pub day: NaiveDate,
    pub purpose: Option<String>,
    pub status: VisitStatus,
}

#[derive(Debug, Clone)]
pub struct Visits {
    pool: SqlitePool,
}

const VISIT_HISTORY_DAYS: i32 = 30;
const VISITS_CLEANUP_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60); // 4 hours

impl Visits {
    pub fn new(pool: SqlitePool) -> Result<Visits> {
        Ok(Visits { pool })
    }

    pub async fn run(self) {
        let ct = CancellationToken::new();
        let ct_wait = ct.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            ct_wait.cancel();
        });
        self.cleanup_loop(ct).await;
    }

    async fn cleanup_loop(&self, ct: CancellationToken) {
        let mut interval = tokio::time::interval(VISITS_CLEANUP_INTERVAL);

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = ct.cancelled() => { break }
            };
            log::debug!("Visits cleanup task running");
            self.cleanup(crate::utils::now())
                .await
                .expect("successful cleanup");
        }
    }

    pub async fn get_visits(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Visit>> {
        let from_day = from.num_days_from_ce();
        let to_day: i32 = to.num_days_from_ce();
        Ok(sqlx::query!(
            "SELECT person, day, purpose, status FROM visit WHERE day >= ?1 AND day <= ?2",
            from_day,
            to_day,
        )
        .map(|r| {
            let day = chrono::NaiveDate::from_num_days_from_ce_opt(r.day as i32).unwrap();
            Visit {
                person: Uid::from(r.person),
                day,
                purpose: r.purpose,
                status: VisitStatus::from(r.status as i32),
            }
        })
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn upsert_visit(&self, visit_update: VisitUpdate) -> Result<bool> {
        let person: i64 = visit_update.person.into();
        let day = visit_update.day.num_days_from_ce();
        let mut tx = self.pool.begin().await?;
        let existing = sqlx::query!(
            "SELECT purpose, status FROM visit WHERE person = ?1 AND day = ?2",
            person,
            day
        )
        .fetch_optional(&mut *tx)
        .await?;
        let changed_status;
        if let Some(row) = existing {
            let should_update_purpose = visit_update.purpose.is_some();
            let should_update_status = visit_update.status != VisitStatus::from(row.status as i32);
            if should_update_purpose || should_update_status {
                let purpose = visit_update.purpose.unwrap_or(row.purpose);
                let status_int: i32 = visit_update.status.into();
                sqlx::query!(
                    "UPDATE visit SET purpose = ?3, status = ?4 WHERE person = ?1 AND day = ?2",
                    person,
                    day,
                    purpose,
                    status_int,
                )
                .execute(&mut *tx)
                .await?;
            }
            changed_status = should_update_status;
        } else {
            let purpose = visit_update.purpose.unwrap_or_default();
            let status_int: i32 = visit_update.status.into();
            sqlx::query!(
                "INSERT INTO visit (person, day, purpose, status) VALUES (?1, ?2, ?3, ?4)",
                person,
                day,
                purpose,
                status_int
            )
            .execute(&mut *tx)
            .await?;
            changed_status = true;
        }
        tx.commit().await?;
        Ok(changed_status)
    }

    pub async fn check_out_everybody(&self, day: NaiveDate) -> Result<()> {
        let day = day.num_days_from_ce();
        let status_int: i32 = VisitStatus::CheckedOut.into();
        sqlx::query!(
            "UPDATE visit SET status = ?1 WHERE day = ?2",
            status_int,
            day,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
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

    pub async fn cleanup(&self, now: impl Datelike) -> Result<()> {
        let current_day = now.num_days_from_ce();
        let cutoff = current_day - VISIT_HISTORY_DAYS;

        sqlx::query!("DELETE FROM visit WHERE day < ?1", cutoff)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

impl From<VisitStatus> for i32 {
    fn from(status: VisitStatus) -> Self {
        match status {
            VisitStatus::Planned => 0,
            VisitStatus::CheckedIn => 1,
            VisitStatus::CheckedOut => 2,
        }
    }
}

impl From<i32> for VisitStatus {
    fn from(val: i32) -> Self {
        match val {
            1 => VisitStatus::CheckedIn,
            2 => VisitStatus::CheckedOut,
            _ => VisitStatus::Planned,
        }
    }
}
