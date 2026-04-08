use sqlx::SqlitePool;

use crate::error::Result;

const INIT_SQL: &str = include_str!("../../migrations/001_init.sql");
const IMPORTS_SQL: &str = include_str!("../../migrations/003_imports_table.sql");
const OVERWRITES_SQL: &str = include_str!("../../migrations/004_import_overwrites.sql");

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::raw_sql(INIT_SQL).execute(pool).await?;

    // Migration 002: add extraction metadata columns to reports (legacy, kept for compat)
    let columns = [
        ("model_used", "TEXT"),
        ("agent_turns", "INTEGER DEFAULT 0"),
        ("extracted_count", "INTEGER DEFAULT 0"),
        ("unresolved_count", "INTEGER DEFAULT 0"),
    ];
    for (col_name, col_type) in &columns {
        let exists: bool = sqlx::query_scalar::<_, i32>(
            &format!("SELECT COUNT(*) FROM pragma_table_info('reports') WHERE name = '{}'", col_name)
        )
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);
        if !exists {
            let sql = format!("ALTER TABLE reports ADD COLUMN {} {}", col_name, col_type);
            sqlx::raw_sql(&sql).execute(pool).await?;
        }
    }

    // Migration 003: imports table
    sqlx::raw_sql(IMPORTS_SQL).execute(pool).await?;

    // Migration 003b: add import_id to observations (if not present)
    let has_import_id: bool = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'import_id'"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false);
    if !has_import_id {
        sqlx::raw_sql("ALTER TABLE observations ADD COLUMN import_id INTEGER REFERENCES imports(id)")
            .execute(pool)
            .await?;
    }

    // Migration 004: import_overwrites table
    sqlx::raw_sql(OVERWRITES_SQL).execute(pool).await?;

    tracing::info!("Database migrations applied");
    Ok(())
}
