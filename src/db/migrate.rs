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

    // Migration 005: fix calculated biomarker LOINC codes (fake -> real)
    let loinc_fixes = [
        ("T.Chol/HDL", "32309-7"),
        ("A/G", "1759-0"),
        ("eGFR", "98979-8"),
        ("TG/HDL", "44733-4"),
    ];
    for (old_code, new_code) in &loinc_fixes {
        // Only migrate if old code exists and new code doesn't
        let old_exists: bool = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM biomarkers WHERE loinc_code = ?"
        )
        .bind(old_code)
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

        let new_exists: bool = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM biomarkers WHERE loinc_code = ?"
        )
        .bind(new_code)
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

        if old_exists && !new_exists {
            sqlx::query("UPDATE biomarkers SET loinc_code = ? WHERE loinc_code = ?")
                .bind(new_code)
                .bind(old_code)
                .execute(pool)
                .await?;
            tracing::info!("Migrated biomarker LOINC code {} -> {}", old_code, new_code);
        }

        // Also fix observations referencing the old code via their biomarker_id
        // (observations point to biomarker by id, so they follow automatically)

        // Fix raw_extraction JSON in imports
        let imports_with_old: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, raw_extraction FROM imports WHERE raw_extraction LIKE ?"
        )
        .bind(format!("%{}%", old_code))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (import_id, json) in &imports_with_old {
            let updated = json.replace(old_code, new_code);
            sqlx::query("UPDATE imports SET raw_extraction = ? WHERE id = ?")
                .bind(&updated)
                .bind(import_id)
                .execute(pool)
                .await?;
            tracing::info!("Updated import {} extraction JSON: {} -> {}", import_id, old_code, new_code);
        }
    }

    // Migration 006: add started_at and completed_at columns to imports
    for col in ["started_at", "completed_at"] {
        let exists: bool = sqlx::query_scalar::<_, i32>(
            &format!("SELECT COUNT(*) FROM pragma_table_info('imports') WHERE name = '{}'", col)
        )
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);
        if !exists {
            sqlx::raw_sql(&format!("ALTER TABLE imports ADD COLUMN {} TEXT", col))
                .execute(pool)
                .await?;
        }
    }

    // Migration 007: add llm_log column to imports
    let has_llm_log: bool = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('imports') WHERE name = 'llm_log'"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false);
    if !has_llm_log {
        sqlx::raw_sql("ALTER TABLE imports ADD COLUMN llm_log TEXT")
            .execute(pool)
            .await?;
    }

    // Migration 008: add text_value column to observations for qualitative results
    let has_text_value: bool = sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'text_value'"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false);
    if !has_text_value {
        sqlx::raw_sql("ALTER TABLE observations ADD COLUMN text_value TEXT")
            .execute(pool)
            .await?;
    }

    tracing::info!("Database migrations applied");
    Ok(())
}
