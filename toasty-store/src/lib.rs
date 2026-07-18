//! [Toasty](https://github.com/tokio-rs/toasty) session store for
//! [`tower-sessions`](https://github.com/maxcountryman/tower-sessions).
//!
//! One store implementation, many backends: SQLite, PostgreSQL, MySQL, and
//! Turso — the store uses only portable Toasty query-builder calls, so every
//! SQL backend shares the same code path. DynamoDB is not supported (the
//! store relies on interactive transactions, which Toasty offers only on SQL
//! backends).
//!
//! # Usage
//!
//! With a store-owned database (the store builds its own `toasty::Db`):
//!
//! ```rust,no_run
//! use tower_sessions_toasty_store::ToastyStore;
//!
//! # tokio_test::block_on(async {
//! let store = ToastyStore::connect("sqlite::memory:").await.unwrap();
//! store.migrate().await.unwrap();
//! # })
//! ```
//!
//! With an application-owned [`toasty::Db`], sharing its connection pool.
//! **The Db must register [`TowerSession`]** — using an unregistered model
//! panics inside Toasty:
//!
//! ```rust,no_run
//! use tower_sessions_toasty_store::{ToastyStore, TowerSession};
//!
//! # tokio_test::block_on(async {
//! let db = toasty::Db::builder()
//!     // your own models join the list: toasty::models!(TowerSession, crate::*)
//!     .models(toasty::models!(TowerSession))
//!     .connect("sqlite::memory:")
//!     .await
//!     .unwrap();
//! let store = ToastyStore::new(db);
//! store.migrate().await.unwrap();
//! # })
//! ```

use async_trait::async_trait;
use time::OffsetDateTime;
pub use toasty;
use tower_sessions_core::{
    session::{Id, Record},
    session_store, ExpiredDeletion, SessionStore,
};

/// The session model, mapped to the `tower_sessions` table.
///
/// Public so applications that bring their own [`toasty::Db`] can register it:
/// `toasty::models!(tower_sessions_toasty_store::TowerSession, crate::*)`.
/// To rename the table at runtime, use
/// `toasty::Db::builder().table_name_prefix(..)` — Toasty fixes the base table
/// name at compile time.
#[derive(Debug, toasty::Model)]
#[table = "tower_sessions"]
pub struct TowerSession {
    #[key]
    id: String,

    /// The full `Record`, encoded with `rmp-serde`.
    data: Vec<u8>,

    /// Unix seconds. An `i64` column works on every Toasty backend;
    /// `time::OffsetDateTime` is not a Toasty field type.
    expiry_date: i64,
}

/// An error type for Toasty stores.
#[derive(thiserror::Error, Debug)]
pub enum ToastyStoreError {
    /// A variant to map `toasty` errors.
    #[error(transparent)]
    Toasty(#[from] toasty::Error),

    /// A variant to map `rmp_serde` encode errors.
    #[error(transparent)]
    Encode(#[from] rmp_serde::encode::Error),

    /// A variant to map `rmp_serde` decode errors.
    #[error(transparent)]
    Decode(#[from] rmp_serde::decode::Error),
}

impl From<ToastyStoreError> for session_store::Error {
    fn from(err: ToastyStoreError) -> Self {
        match err {
            ToastyStoreError::Toasty(inner) => session_store::Error::Backend(inner.to_string()),
            ToastyStoreError::Decode(inner) => session_store::Error::Decode(inner.to_string()),
            ToastyStoreError::Encode(inner) => session_store::Error::Encode(inner.to_string()),
        }
    }
}

/// A Toasty session store.
#[derive(Clone, Debug)]
pub struct ToastyStore {
    db: toasty::Db,
}

impl ToastyStore {
    /// Create a new Toasty store from an application-owned [`toasty::Db`].
    ///
    /// The store clones the handle per operation; clones share the underlying
    /// connection pool, so this is cheap. The `Db` **must** have
    /// [`TowerSession`] registered in its `toasty::models!` list — Toasty
    /// panics on queries for unregistered models.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tower_sessions_toasty_store::{ToastyStore, TowerSession};
    ///
    /// # tokio_test::block_on(async {
    /// let db = toasty::Db::builder()
    ///     .models(toasty::models!(TowerSession))
    ///     .connect("sqlite::memory:")
    ///     .await
    ///     .unwrap();
    /// let session_store = ToastyStore::new(db);
    /// # })
    /// ```
    pub fn new(db: toasty::Db) -> Self {
        Self { db }
    }

    /// Create a store with its own [`toasty::Db`] (and connection pool) from a
    /// connection URL, registering only the session model.
    ///
    /// The URL scheme selects the driver; enable the matching `toasty` cargo
    /// feature (`sqlite`, `postgresql`, `mysql`, `turso`) in your application.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tower_sessions_toasty_store::ToastyStore;
    ///
    /// # tokio_test::block_on(async {
    /// let session_store = ToastyStore::connect("sqlite::memory:").await.unwrap();
    /// # })
    /// ```
    pub async fn connect(url: impl AsRef<str>) -> toasty::Result<Self> {
        let db = toasty::Db::builder()
            .models(toasty::models!(TowerSession))
            .connect(url.as_ref())
            .await?;
        Ok(Self::new(db))
    }

    /// Migrate the session schema. Idempotent — safe to call on every start.
    ///
    /// `push_schema` fails if tables already exist, so this probes the session
    /// table with a cheap query first and only pushes when the probe errors.
    /// With an application-owned `Db`, `push_schema` pushes the *whole*
    /// registered schema; applications managing their schema with toasty-cli
    /// migrations should include [`TowerSession`] there and skip `migrate`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tower_sessions_toasty_store::ToastyStore;
    ///
    /// # tokio_test::block_on(async {
    /// let session_store = ToastyStore::connect("sqlite::memory:").await.unwrap();
    /// session_store.migrate().await.unwrap();
    /// # })
    /// ```
    pub async fn migrate(&self) -> toasty::Result<()> {
        let mut db = self.db.clone();
        if self.probe(&mut db).await.is_ok() {
            return Ok(());
        }
        if let Err(push_err) = db.push_schema().await {
            // A concurrent migrate may have pushed between probe and push. If
            // the table exists now, the schema is in place and this succeeded.
            if self.probe(&mut db).await.is_ok() {
                return Ok(());
            }
            return Err(push_err);
        }
        Ok(())
    }

    async fn probe(&self, db: &mut toasty::Db) -> toasty::Result<()> {
        TowerSession::filter_by_id("")
            .first()
            .exec(db)
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl ExpiredDeletion for ToastyStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        let mut db = self.db.clone();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        TowerSession::filter(TowerSession::fields().expiry_date().lt(now))
            .delete()
            .exec(&mut db)
            .await
            .map_err(ToastyStoreError::Toasty)?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for ToastyStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(ToastyStoreError::Toasty)?;

        while TowerSession::filter_by_id(record.id.to_string())
            .first()
            .exec(&mut tx)
            .await
            .map_err(ToastyStoreError::Toasty)?
            .is_some()
        {
            record.id = Id::default();
        }

        let data = rmp_serde::to_vec(record).map_err(ToastyStoreError::Encode)?;

        toasty::create!(TowerSession {
            id: record.id.to_string(),
            data,
            expiry_date: record.expiry_date.unix_timestamp(),
        })
        .exec(&mut tx)
        .await
        .map_err(ToastyStoreError::Toasty)?;

        tx.commit().await.map_err(ToastyStoreError::Toasty)?;
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let mut db = self.db.clone();
        let id = record.id.to_string();
        let data = rmp_serde::to_vec(record).map_err(ToastyStoreError::Encode)?;
        let expiry_date = record.expiry_date.unix_timestamp();

        // Upsert. Toasty has no native upsert and updates don't report whether
        // a row matched, so: select inside a transaction, then branch.
        let mut tx = db.transaction().await.map_err(ToastyStoreError::Toasty)?;

        let exists = TowerSession::filter_by_id(&id)
            .first()
            .exec(&mut tx)
            .await
            .map_err(ToastyStoreError::Toasty)?
            .is_some();

        if exists {
            TowerSession::update_by_id(&id)
                .data(data)
                .expiry_date(expiry_date)
                .exec(&mut tx)
                .await
                .map_err(ToastyStoreError::Toasty)?;
            tx.commit().await.map_err(ToastyStoreError::Toasty)?;
        } else {
            let created = toasty::create!(TowerSession {
                id: &id,
                data: data.clone(),
                expiry_date,
            })
            .exec(&mut tx)
            .await;

            match created {
                Ok(_) => {
                    tx.commit().await.map_err(ToastyStoreError::Toasty)?;
                }
                Err(_) => {
                    // A concurrent first-save of the same id won the insert.
                    // Roll back and apply this save as the update it now is.
                    drop(tx);
                    TowerSession::update_by_id(&id)
                        .data(data)
                        .expiry_date(expiry_date)
                        .exec(&mut db)
                        .await
                        .map_err(ToastyStoreError::Toasty)?;
                }
            }
        }

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let mut db = self.db.clone();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let session = TowerSession::filter_by_id(session_id.to_string())
            .filter(TowerSession::fields().expiry_date().gt(now))
            .first()
            .exec(&mut db)
            .await
            .map_err(ToastyStoreError::Toasty)?;

        match session {
            Some(session) => Ok(Some(
                rmp_serde::from_slice(&session.data).map_err(ToastyStoreError::Decode)?,
            )),
            None => Ok(None),
        }
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let mut db = self.db.clone();
        TowerSession::delete_by_id(&mut db, session_id.to_string())
            .await
            .map_err(ToastyStoreError::Toasty)?;
        Ok(())
    }
}

// unit tests following tower-sessions-rusqlite-store, which took them from
// https://github.com/maxcountryman/tower-sessions/blob/6ad8933b4f5e71f3202f0c1a28f194f3db5234c8/memory-store/src/lib.rs#L62
#[cfg(test)]
mod toasty_store_tests {
    use time::Duration;

    use super::*;

    async fn create_store() -> ToastyStore {
        let store = ToastyStore::connect("sqlite::memory:").await.unwrap();
        store.migrate().await.unwrap();
        store
    }

    fn test_record() -> Record {
        Record {
            id: Default::default(),
            data: Default::default(),
            expiry_date: OffsetDateTime::now_utc() + Duration::minutes(30),
        }
    }

    #[tokio::test]
    async fn test_create() {
        let store = create_store().await;
        let mut record = test_record();
        assert!(store.create(&mut record).await.is_ok());
    }

    #[tokio::test]
    async fn test_save() {
        let store = create_store().await;
        let record = test_record();
        assert!(store.save(&record).await.is_ok());
    }

    #[tokio::test]
    async fn test_save_insert_then_update() {
        let store = create_store().await;
        let mut record = test_record();

        // insert path: id was never created
        store.save(&record).await.unwrap();
        assert_eq!(Some(record.clone()), store.load(&record.id).await.unwrap());

        // update path: data actually changes
        record
            .data
            .insert("foo".to_string(), serde_json::to_value(42).unwrap());
        store.save(&record).await.unwrap();
        assert_eq!(Some(record.clone()), store.load(&record.id).await.unwrap());
    }

    #[tokio::test]
    async fn test_load() {
        let store = create_store().await;
        let mut record = test_record();
        store.create(&mut record).await.unwrap();
        let loaded_record = store.load(&record.id).await.unwrap();
        assert_eq!(Some(record), loaded_record);
    }

    #[tokio::test]
    async fn test_load_expired_returns_none() {
        let store = create_store().await;
        let mut record = test_record();
        record.expiry_date = OffsetDateTime::now_utc() - Duration::minutes(30);
        store.create(&mut record).await.unwrap();
        assert_eq!(None, store.load(&record.id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = create_store().await;
        let mut record = test_record();
        store.create(&mut record).await.unwrap();
        assert!(store.delete(&record.id).await.is_ok());
        assert_eq!(None, store.load(&record.id).await.unwrap());
    }

    #[tokio::test]
    async fn test_create_id_collision() {
        let store = create_store().await;
        let mut record1 = test_record();
        let mut record2 = test_record();
        store.create(&mut record1).await.unwrap();
        record2.id = record1.id; // Set the same ID for record2
        store.create(&mut record2).await.unwrap();
        assert_ne!(record1.id, record2.id); // IDs should be different
    }

    #[tokio::test]
    async fn test_delete_expired() {
        let store = create_store().await;
        let mut record = test_record();
        record.expiry_date = OffsetDateTime::now_utc() - Duration::minutes(30);
        store.create(&mut record).await.unwrap();
        store.delete_expired().await.unwrap();
        assert_eq!(None, store.load(&record.id).await.unwrap());
    }

    #[tokio::test]
    async fn test_migrate_idempotent() {
        let store = ToastyStore::connect("sqlite::memory:").await.unwrap();
        store.migrate().await.unwrap();
        store.migrate().await.unwrap();

        // still fully functional
        let mut record = test_record();
        store.create(&mut record).await.unwrap();
        assert_eq!(Some(record.clone()), store.load(&record.id).await.unwrap());
    }

    #[derive(Debug, toasty::Model)]
    struct OtherThing {
        #[key]
        #[auto]
        id: u64,
        name: String,
    }

    #[tokio::test]
    async fn test_shared_db_with_user_model_and_prefix() {
        let db = toasty::Db::builder()
            .models(toasty::models!(TowerSession, OtherThing))
            .table_name_prefix("myapp_")
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let store = ToastyStore::new(db.clone());
        store.migrate().await.unwrap();

        let mut record = test_record();
        store.create(&mut record).await.unwrap();
        assert_eq!(Some(record.clone()), store.load(&record.id).await.unwrap());

        // the user model coexists on the same Db
        let mut db = db;
        let thing = toasty::create!(OtherThing { name: "thing" })
            .exec(&mut db)
            .await
            .unwrap();
        assert_eq!(thing.name, "thing");
    }
}
