use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

pub async fn connect(url: &str) -> Result<SqlitePool> {
  let pool = SqlitePoolOptions::new()
    .max_connections(5)
    .connect(url)
    .await?;
  init_schema(&pool).await?;
  Ok(pool)
}

pub async fn init_schema(pool: &SqlitePool) -> Result<()> {
  sqlx::query(
    "CREATE TABLE IF NOT EXISTS users (
       id            INTEGER PRIMARY KEY AUTOINCREMENT,
       email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
       password_hash TEXT NOT NULL,
       role          TEXT NOT NULL DEFAULT 'admin',
       created_at    INTEGER NOT NULL
     )",
  )
  .execute(pool)
  .await?;
  Ok(())
}
