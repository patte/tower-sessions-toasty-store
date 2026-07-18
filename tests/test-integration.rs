#[macro_use]
mod common;

#[cfg(test)]
mod toasty_sqlite_store_tests {
    use axum::Router;
    use tower_sessions::SessionManagerLayer;
    use tower_sessions_toasty_store::ToastyStore;

    use crate::common::build_app;

    async fn app(max_age: Option<Duration>) -> Router {
        let session_store = ToastyStore::connect("sqlite::memory:").await.unwrap();
        session_store.migrate().await.unwrap();
        let session_manager = SessionManagerLayer::new(session_store).with_secure(true);

        build_app(session_manager, max_age)
    }

    route_tests!(app);
}

#[cfg(test)]
mod toasty_turso_store_tests {
    use axum::Router;
    use tower_sessions::SessionManagerLayer;
    use tower_sessions_toasty_store::ToastyStore;

    use crate::common::build_app;

    async fn app(max_age: Option<Duration>) -> Router {
        let session_store = ToastyStore::connect("turso::memory:").await.unwrap();
        session_store.migrate().await.unwrap();
        let session_manager = SessionManagerLayer::new(session_store).with_secure(true);

        build_app(session_manager, max_age)
    }

    route_tests!(app);
}

#[cfg(all(test, feature = "postgresql"))]
mod toasty_postgres_store_tests {
    use axum::Router;
    use tower_sessions::SessionManagerLayer;
    use tower_sessions_toasty_store::ToastyStore;

    use crate::common::build_app;

    async fn app(max_age: Option<Duration>) -> Router {
        let database_url = std::option_env!("POSTGRES_URL").unwrap();
        let session_store = ToastyStore::connect(database_url).await.unwrap();
        session_store.migrate().await.unwrap();
        let session_manager = SessionManagerLayer::new(session_store).with_secure(true);

        build_app(session_manager, max_age)
    }

    route_tests!(app);
}

#[cfg(all(test, feature = "mysql"))]
mod toasty_mysql_store_tests {
    use axum::Router;
    use tower_sessions::SessionManagerLayer;
    use tower_sessions_toasty_store::ToastyStore;

    use crate::common::build_app;

    async fn app(max_age: Option<Duration>) -> Router {
        let database_url = std::option_env!("MYSQL_URL").unwrap();
        let session_store = ToastyStore::connect(database_url).await.unwrap();
        session_store.migrate().await.unwrap();
        let session_manager = SessionManagerLayer::new(session_store).with_secure(true);

        build_app(session_manager, max_age)
    }

    route_tests!(app);
}
