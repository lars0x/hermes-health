# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
make serve          # Start web server (127.0.0.1:8080)
make test           # Run all tests (45 tests, uses in-memory SQLite)
make build          # Build
make clean          # Wipe database + uploaded reports
make reset          # clean + serve

cargo run -- init                    # Seed biomarkers into fresh DB
cargo run -- serve                   # Start server (auto-seeds if empty)
cargo run -- obs add "HDL" 52 "mg/dL" --date 2026-03-15 --fasting true
cargo run -- trend "Total Cholesterol" --window 365
cargo run -- search "hemoglobin"     # Search LOINC catalog
cargo run -- export csv --output export.csv

cargo test --test integration_test test_trend_analysis   # Single test
RUST_LOG=hermes_health=debug cargo run -- serve          # Debug logging
```

## Architecture

Single Rust binary: CLI + web server sharing the same service layer.

```
CLI (clap)  ─┐
              ├── services/ (business logic) ── db/ (sqlx + SQLite)
Web (axum)  ─┘         │
                   ingest/ (unit conversion, precision)
                   agent/ (Ollama LLM extraction)
```

**Web layer**: Axum 0.8 handlers return `Result<Html<String>, HermesError>`. Templates rendered via Minijinja. HTMX for partial page updates (detect via `htmx::is_htmx_request(&headers)`, control with `is_fragment` in template context). Static assets served via `rust-embed`.

**Data model**: `Report` (uploaded file) -> `Import` (extraction run, tracks model + status) -> `Observation` (committed result) -> `Biomarker` (LOINC-coded marker with ranges). One report can have multiple imports (different models/retries).

**LLM extraction pipeline** (`src/agent/extractor.rs`): Three parallel Ollama calls:
1. Extract biomarkers from PDF text (main call, ~2 min)
2. Resolve unmatched markers against tracked biomarkers (LLM mapping)
3. Extract test/specimen collection date

PDF text extraction: tries Rust `pdf-extract` first, falls back to Python `pypdf` for encrypted PDFs (common in Singapore lab reports).

**LOINC matching priority**: tracked biomarker name/alias (exact) -> LOINC catalog fuzzy match (Jaro-Winkler >= 0.80) -> LLM resolution -> unresolved.

**Unit conversion**: `ingest/normalize.rs` handles canonical unit conversion with precision preservation. Conversion factors stored in `unit_conversions` table per biomarker. Original value and unit always preserved alongside canonical.

**Trend analysis**: `services/trend.rs` computes OLS linear regression, direction (increasing/decreasing/stable), contextual status (improving/worsening based on optimal ranges), and alerts (approaching_limit, rapid_change, reversal).

## Database

SQLite via sqlx (runtime queries, not compile-time checked). Migrations in `migrations/` are embedded and run on startup via `src/db/migrate.rs`. The migration runner handles idempotent column additions for ALTER TABLE statements.

Key tables: `biomarkers`, `observations`, `reports`, `imports`, `interventions`, `unit_conversions`.

## Config

`config.toml` at project root. All sections have defaults (server runs without config file). Ollama server URL and model configured in `[ollama]` section. Extraction mode (`agentic` or `direct`) in `[extraction]`.

## Spec and Wireframes

Full application spec at `docs/SPEC.md`. HTML wireframes at `docs/wireframes/`. These are the source of truth for features and visual design.

## Key Conventions

- Biomarkers identified by LOINC codes. Full LOINC 2.81 lab catalog (59K entries) embedded via `rust-embed` from `data/loinc_core.csv` and `data/loinc_aliases.tsv`.
- 49 seeded biomarkers with preconfigured reference/optimal ranges and Singapore lab aliases in `src/services/seed.rs`.
- Templates in `templates/` directory (Minijinja). Pages extend `base.html`. Components are includable fragments.
- Static JS/CSS in `src/web/static/` (HTMX, uPlot, hermes.css, hermes.js). Cache headers set to no-cache in dev.
- CSS design tokens match wireframe exactly (see `:root` in `hermes.css`).
- Axum route ordering: literal paths before parameterized (e.g., `/api/v1/biomarkers/search` before `/api/v1/biomarkers/{id}`).
