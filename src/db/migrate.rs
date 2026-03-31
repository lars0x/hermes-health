use sqlx::SqlitePool;

use crate::error::Result;

const INIT_SQL: &str = include_str!("../../migrations/001_init.sql");

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::raw_sql(INIT_SQL).execute(pool).await?;
    tracing::info!("Database migrations applied");
    Ok(())
}
