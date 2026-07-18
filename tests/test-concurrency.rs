//! Concurrent stress tests for the store's contended write paths.
//!
//! The route tests exercise every backend sequentially; these run the
//! contended paths in parallel: racing first-saves of one id (the
//! insert-race fallback), racing updates of one session, and bulk parallel
//! creates. File-backed SQLite matters here — unlike `sqlite::memory:` its
//! pool holds multiple connections, so real write-write contention occurs.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};
use tower_sessions::{
    session::{Id, Record},
    SessionStore,
};
use tower_sessions_toasty_store::ToastyStore;

const TASKS: usize = 16;

fn record_with(id: Id, marker: usize) -> Record {
    Record {
        id,
        data: HashMap::from([("task".to_string(), serde_json::json!(marker))]),
        expiry_date: OffsetDateTime::now_utc() + Duration::minutes(30),
    }
}

async fn connect(url: &str) -> ToastyStore {
    let store = ToastyStore::connect(url).await.unwrap();
    store.migrate().await.unwrap();
    store
}

pub async fn concurrent_creates(store: ToastyStore) {
    let mut handles = Vec::new();
    for i in 0..TASKS {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            let mut record = record_with(Id::default(), i);
            store.create(&mut record).await.unwrap();
            assert_eq!(Some(record.clone()), store.load(&record.id).await.unwrap());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
}

pub async fn concurrent_first_saves_same_id(store: ToastyStore) {
    let id = Id::default();
    let mut handles = Vec::new();
    for i in 0..TASKS {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store.save(&record_with(id, i)).await.unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    // one of the racing saves won; the stored record must decode cleanly
    let loaded = store.load(&id).await.unwrap().expect("session must exist");
    assert_eq!(loaded.id, id);
    assert!(loaded.data.contains_key("task"));
}

pub async fn concurrent_updates_same_session(store: ToastyStore) {
    let mut record = record_with(Id::default(), 0);
    store.create(&mut record).await.unwrap();

    let id = record.id;
    let mut handles = Vec::new();
    for i in 1..=TASKS {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store.save(&record_with(id, i)).await.unwrap();
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    let loaded = store.load(&id).await.unwrap().expect("session must exist");
    assert_eq!(loaded.id, id);
}

macro_rules! stress_tests {
    ($url:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        async fn concurrent_creates() {
            crate::concurrent_creates(crate::connect(&$url("creates")).await).await;
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        async fn concurrent_first_saves_same_id() {
            crate::concurrent_first_saves_same_id(crate::connect(&$url("first_saves")).await).await;
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        async fn concurrent_updates_same_session() {
            crate::concurrent_updates_same_session(crate::connect(&$url("updates")).await).await;
        }
    };
}

mod sqlite_file_stress_tests {
    // one db file per test: contention should come from the tasks inside a
    // test, not from unrelated tests sharing a file
    fn url(name: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "tower_sessions_toasty_stress_{}_{name}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        format!("sqlite:{}", path.display())
    }

    stress_tests!(url);
}

mod turso_stress_tests {
    // in-memory turso is shared across all pool connections, so it is
    // concurrency-capable without touching disk
    fn url(_name: &str) -> String {
        "turso::memory:".to_string()
    }

    stress_tests!(url);
}

#[cfg(feature = "postgresql")]
mod postgres_stress_tests {
    fn url(_name: &str) -> String {
        std::option_env!("POSTGRES_URL").unwrap().to_string()
    }

    stress_tests!(url);
}

#[cfg(feature = "mysql")]
mod mysql_stress_tests {
    fn url(_name: &str) -> String {
        std::option_env!("MYSQL_URL").unwrap().to_string()
    }

    stress_tests!(url);
}
