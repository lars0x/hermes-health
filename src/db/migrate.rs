use sqlx::SqlitePool;

use crate::error::Result;

const INIT_SQL: &str = include_str!("../../migrations/001_init.sql");
const EXTRACTION_META_SQL: &str = include_str!("../../migrations/002_extraction_metadata.sql");

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::raw_sql(INIT_SQL).execute(pool).await?;

    // Migration 002: add extraction metadata columns (idempotent via IF NOT EXISTS pattern)
    // SQLite ALTER TABLE ADD COLUMN fails if column already exists, so we check first
    let has_model_used: bool = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('reports') WHERE name = 'model_used'"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false);

    if !has_model_used {
        // Run each ALTER TABLE statement separately (SQLite doesn't support multi-ALTER)
        for statement in EXTRACTION_META_SQL.split(';') {
            let stmt = statement.trim();
            if !stmt.is_empty() && !stmt.starts_with("--") {
                sqlx::raw_sql(stmt).execute(pool).await?;
            }
        }
        tracing::info!("Migration 002 applied: extraction metadata columns");
    }

    tracing::info!("Database migrations applied");
    Ok(())
}
