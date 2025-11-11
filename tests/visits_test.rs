use chrono::NaiveDate;
use sqlx::sqlite::SqlitePool;
use xecut_bot::backend::Uid;
use xecut_bot::backend::connect_db;
use xecut_bot::{Visit, VisitStatus, Visits};

fn in_memory_db_config() -> xecut_bot::config::DbConfig {
    xecut_bot::config::DbConfig {
        sqlite_path: ":memory:".to_string(),
    }
}

async fn setup_schema(pool: &SqlitePool) {
    sqlx::query(
        "
        CREATE TABLE visit (
            person INTEGER,
            day INTEGER,
            purpose TEXT,
            status INTEGER,
            PRIMARY KEY (person, day)
        );
        ",
    )
    .execute(pool)
    .await
    .unwrap();
}

// Helper to create Visits with in-memory DB and apply schema
async fn make_visits() -> Visits {
    let cfg = in_memory_db_config();
    let pool = connect_db(&cfg).await.unwrap();
    let visits = Visits::new(pool.clone()).unwrap();
    setup_schema(&pool).await;
    visits
}

#[tokio::test]
async fn test_upsert_and_get_visits() {
    let visits = make_visits().await;
    let person = Uid::from(1);
    let day = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let update = xecut_bot::visits::VisitUpdate {
        person,
        day,
        purpose: Some("work".to_string()),
        status: VisitStatus::Planned,
    };
    let inserted = visits.upsert_visit(&update).await.unwrap();
    assert!(inserted);
    let visits_vec = visits.get_visits(day, day).await.unwrap();
    assert_eq!(
        visits_vec,
        vec![Visit {
            person,
            day,
            purpose: "work".to_string(),
            status: VisitStatus::Planned,
        }]
    );
}

#[tokio::test]
async fn test_upsert_update_visit() {
    let visits = make_visits().await;
    let person = Uid::from(2);
    let day = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let update1 = xecut_bot::visits::VisitUpdate {
        person,
        day,
        purpose: Some("work".to_string()),
        status: VisitStatus::Planned,
    };
    let updated = visits.upsert_visit(&update1).await.unwrap();
    assert!(updated);
    let update2 = xecut_bot::visits::VisitUpdate {
        person,
        day,
        purpose: Some("meeting".to_string()),
        status: VisitStatus::CheckedIn,
    };
    let updated = visits.upsert_visit(&update2).await.unwrap();
    assert!(updated);
    let update3 = xecut_bot::visits::VisitUpdate {
        person,
        day,
        purpose: Some("meeting".to_string()),
        status: VisitStatus::CheckedIn,
    };
    let updated = visits.upsert_visit(&update3).await.unwrap();
    assert!(!updated);
    let visits_vec = visits.get_visits(day, day).await.unwrap();
    assert_eq!(
        visits_vec,
        vec![Visit {
            person,
            day,
            purpose: "meeting".to_string(),
            status: VisitStatus::CheckedIn,
        }]
    );
}

#[tokio::test]
async fn test_delete_visit() {
    let visits = make_visits().await;
    let person = Uid::from(3);
    let day = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let update = xecut_bot::visits::VisitUpdate {
        person,
        day,
        purpose: Some("delete".to_string()),
        status: VisitStatus::Planned,
    };
    let inserted = visits.upsert_visit(&update).await.unwrap();
    assert!(inserted);
    let deleted = visits.delete_visit(person, day).await.unwrap();
    assert!(deleted);
    let visits_vec = visits.get_visits(day, day).await.unwrap();
    assert_eq!(visits_vec, vec![]);
}

#[tokio::test]
async fn test_delete_nonexistent_visit() {
    let visits = make_visits().await;
    let person = Uid::from(999);
    let day = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let deleted = visits.delete_visit(person, day).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_cleanup() {
    let visits = make_visits().await;
    let person = Uid::from(4);
    let old_day = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    let new_day = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let update_old = xecut_bot::visits::VisitUpdate {
        person,
        day: old_day,
        purpose: Some("old".to_string()),
        status: VisitStatus::Planned,
    };
    let update_new = xecut_bot::visits::VisitUpdate {
        person,
        day: new_day,
        purpose: Some("new".to_string()),
        status: VisitStatus::Planned,
    };
    let inserted = visits.upsert_visit(&update_old).await.unwrap();
    assert!(inserted);
    let inserted = visits.upsert_visit(&update_new).await.unwrap();
    assert!(inserted);
    // Use a fixed date for cleanup instead of chrono::Local::now()
    let cleanup_date = NaiveDate::from_ymd_opt(2025, 8, 8)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let offset = *chrono::Local::now().offset();
    let fixed_datetime =
        chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(cleanup_date, offset);
    visits.cleanup(fixed_datetime).await.unwrap();
    let visits_vec = visits.get_visits(new_day, new_day).await.unwrap();
    assert_eq!(
        visits_vec,
        vec![Visit {
            person,
            day: new_day,
            purpose: "new".to_string(),
            status: VisitStatus::Planned,
        }]
    );
}

#[tokio::test]
async fn test_get_visits_range() {
    let visits = make_visits().await;
    let person1 = Uid::from(10);
    let person2 = Uid::from(11);
    let day1 = NaiveDate::from_ymd_opt(2025, 8, 7).unwrap();
    let day2 = NaiveDate::from_ymd_opt(2025, 8, 8).unwrap();
    let day3 = NaiveDate::from_ymd_opt(2025, 8, 9).unwrap();
    let update1 = xecut_bot::visits::VisitUpdate {
        person: person1,
        day: day1,
        purpose: Some("foo".to_string()),
        status: VisitStatus::Planned,
    };
    let update2 = xecut_bot::visits::VisitUpdate {
        person: person2,
        day: day2,
        purpose: Some("bar".to_string()),
        status: VisitStatus::CheckedIn,
    };
    let inserted = visits.upsert_visit(&update1).await.unwrap();
    assert!(inserted);
    let inserted = visits.upsert_visit(&update2).await.unwrap();
    assert!(inserted);
    // Range covering both days
    let visits_vec = visits.get_visits(day1, day2).await.unwrap();
    assert_eq!(visits_vec.len(), 2);
    assert!(visits_vec.contains(&Visit {
        person: person1,
        day: day1,
        purpose: "foo".to_string(),
        status: VisitStatus::Planned,
    }));
    assert!(visits_vec.contains(&Visit {
        person: person2,
        day: day2,
        purpose: "bar".to_string(),
        status: VisitStatus::CheckedIn,
    }));
    // Single day: day1
    let visits_day1 = visits.get_visits(day1, day1).await.unwrap();
    assert_eq!(
        visits_day1,
        vec![Visit {
            person: person1,
            day: day1,
            purpose: "foo".to_string(),
            status: VisitStatus::Planned,
        }]
    );
    // Single day: day2
    let visits_day2 = visits.get_visits(day2, day2).await.unwrap();
    assert_eq!(
        visits_day2,
        vec![Visit {
            person: person2,
            day: day2,
            purpose: "bar".to_string(),
            status: VisitStatus::CheckedIn,
        }]
    );
    // Range with no visits
    let visits_none = visits.get_visits(day3, day3).await.unwrap();
    assert_eq!(visits_none, vec![]);
}
