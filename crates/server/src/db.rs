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

/// Run embedded migrations (server/migrations/*.sql), tracked in _sqlx_migrations.
pub async fn init_schema(pool: &SqlitePool) -> Result<()> {
  sqlx::migrate!("./migrations").run(pool).await?;
  Ok(())
}
