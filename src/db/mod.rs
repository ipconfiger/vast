use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use std::path::Path;
use std::time::Duration;

const MIN_DISK_FREE: u64 = 100 * 1024 * 1024;

/// Initialize the SQLite connection pool with WAL mode and proper PRAGMAs
pub async fn init_pool(db_path: &Path) -> sqlx::SqlitePool {
    // Ensure the data directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create data directory");
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .pragma("cache_size", "-64000")
        .pragma("mmap_size", "268435456")
        .pragma("temp_store", "memory");

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .expect("Failed to create database pool");

    // Run migrations embedded at compile time
    sqlx::migrate!("db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    log_pragma_values(&pool).await;

    pool
}

async fn log_pragma_values(pool: impl sqlx::Executor<'static, Database = sqlx::Sqlite> + Copy) {
    if let Ok(val) =
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode").fetch_one(pool).await
    {
        tracing::info!("PRAGMA journal_mode = {val}");
    }
    if let Ok(val) =
        sqlx::query_scalar::<_, String>("PRAGMA synchronous").fetch_one(pool).await
    {
        tracing::info!("PRAGMA synchronous = {val}");
    }
    if let Ok(val) =
        sqlx::query_scalar::<_, String>("PRAGMA cache_size").fetch_one(pool).await
    {
        tracing::info!("PRAGMA cache_size = {val}");
    }
    if let Ok(val) =
        sqlx::query_scalar::<_, String>("PRAGMA mmap_size").fetch_one(pool).await
    {
        tracing::info!("PRAGMA mmap_size = {val}");
    }
    if let Ok(val) =
        sqlx::query_scalar::<_, String>("PRAGMA busy_timeout").fetch_one(pool).await
    {
        tracing::info!("PRAGMA busy_timeout = {val}");
    }
}

pub async fn shutdown(pool: &sqlx::SqlitePool) {
    tracing::info!("Running PRAGMA optimize...");
    if let Err(e) = sqlx::query("PRAGMA optimize").execute(pool).await {
        tracing::warn!("PRAGMA optimize failed: {e}");
    }
    pool.close().await;
    tracing::info!("Database pool closed.");
}

pub fn check_disk_space(path: &Path) -> Result<(), String> {
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf()
    };

    match fs2::available_space(&dir) {
        Ok(avail) if avail < MIN_DISK_FREE => {
            let mb = avail / (1024 * 1024);
            Err(format!(
                "Insufficient disk space: only {mb} MB available, need at least 100 MB"
            ))
        }
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!("Could not check disk space: {e} — proceeding anyway");
            Ok(())
        }
    }
}
