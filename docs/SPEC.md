# Hermes Health — Application Specification

**Version:** 0.2.0-draft
**Date:** 2026-03-31
**Author:** Lars

---

## 1. Vision

Hermes Health is a local-first, Rust-based biomarker tracking application for healthspan and longevity monitoring. It replaces ad hoc spreadsheet and Streamlit-based workflows with a single, performant binary that owns your data, complies with clinical standards (LOINC), and uses a local LLM (Ollama) to ingest unstructured lab reports.

### Design Principles

- **Local-first, zero cloud dependency** — all data stored in a local SQLite database; no external accounts or SaaS required.
- **Standards-compliant** — LOINC codes as the canonical identifier for all biomarkers; FHIR R4 export for interoperability.
- **Single binary** — `cargo build --release` produces one artifact that embeds migrations, static assets, and the LOINC catalog subset.
- **Transparent data model** — the SQLite file is yours; query it with any tool.

---

## 2. Domain Model

### 2.1 Biomarkers

A **Biomarker** is a measurable health indicator identified by a LOINC code.

| Field | Type | Description |
|---|---|---|
| `id` | `i64` | Internal PK (autoincrement) |
| `loinc_code` | `String` | LOINC identifier (e.g. `2093-3` for Total Cholesterol) |
| `name` | `String` | Canonical display name from LOINC |
| `aliases` | `Vec<String>` | Alternate names encountered in lab reports (e.g. "TC", "Total Chol") |
| `unit` | `String` | UCUM unit (e.g. `mg/dL`, `nmol/L`) |
| `category` | `String` | Grouping (e.g. Lipid Panel, Metabolic, Hormonal, Inflammatory) |
| `reference_low` | `Option<f64>` | Lower bound of reference range |
| `reference_high` | `Option<f64>` | Upper bound of reference range |
| `optimal_low` | `Option<f64>` | Lower bound of *optimal* range (longevity-oriented, user-configurable) |
| `optimal_high` | `Option<f64>` | Upper bound of *optimal* range |

**Notes:**

- Reference ranges can be age- and sex-dependent. The initial version stores a single user-configurable range per marker. A future version may introduce demographic-aware ranges.
- Optimal ranges reflect longevity targets (e.g. Peter Attia / Bryan Johnson protocols) and are distinct from conventional lab ranges. These are user-editable.
- Calculated markers (e.g. TG/HDL ratio, eGFR) are stored as biomarkers with a `source = "calculated"` flag and an associated formula expression.

### 2.2 Observations (Lab Results)

An **Observation** is a single measured or calculated value for a biomarker at a point in time.

| Field | Type | Description |
|---|---|---|
| `id` | `i64` | Internal PK |
| `biomarker_id` | `i64` | FK → Biomarker |
| `value` | `f64` | Measured value, **normalized to the biomarker's canonical unit** |
| `original_value` | `String` | Value exactly as reported by the lab (string-preserving, e.g. `"5.20"`) |
| `original_unit` | `String` | Unit exactly as reported by the lab |
| `precision` | `u8` | Number of decimal places in the original value (derived from `original_value`) |
| `observed_at` | `NaiveDate` | Date of blood draw / test |
| `lab_name` | `Option<String>` | Laboratory that performed the test |
| `report_id` | `Option<i64>` | FK → Report (if imported from a file) |
| `fasting` | `Option<bool>` | Whether the draw was fasted |
| `notes` | `Option<String>` | Free-text (e.g. "day 3 of extended fast") |
| `detection_limit` | `Option<String>` | `"<"`, `">"`, or null — indicates value is below/above instrument range |
| `created_at` | `DateTime<Utc>` | Record creation timestamp |

**Precision & unit rules:**

- `original_value` and `original_unit` preserve the lab report verbatim — they are never modified after initial storage.
- `value` is always in the biomarker's canonical unit (see §2.1 `unit` field). If the lab reported in a different unit, conversion happens at write time and the conversion factor is auditable via `original_value` / `original_unit`.
- `precision` is auto-derived: `"5.2"` → 1, `"5.20"` → 2, `"185"` → 0. Display formatting uses this to avoid showing false precision (e.g. a converted value should not display more decimal places than the original measurement warrants).
- When the original unit matches the canonical unit, `value` = parsed `original_value` with no conversion.

### 2.3 Reports (Uploaded Lab Files)

A **Report** represents an uploaded lab document (PDF or CSV).

| Field | Type | Description |
|---|---|---|
| `id` | `i64` | Internal PK |
| `filename` | `String` | Original filename |
| `file_hash` | `String` | SHA-256 of original file (deduplication) |
| `file_path` | `String` | Path to stored copy in `data/reports/` |
| `format` | `String` | `pdf` or `csv` |
| `imported_at` | `DateTime<Utc>` | Timestamp of import |
| `extraction_status` | `String` | `pending`, `extracted`, `reviewed`, `failed` |
| `raw_extraction` | `Option<String>` | JSON blob of Ollama's raw extraction output |

### 2.4 Interventions

An **Intervention** is anything the user does to influence biomarkers — supplements, medications, diet protocols, exercise regimens, fasting.

| Field | Type | Description |
|---|---|---|
| `id` | `i64` | Internal PK |
| `name` | `String` | e.g. "NMN (Uthever) 500mg" |
| `category` | `String` | `supplement`, `medication`, `diet`, `exercise`, `fasting`, `other` |
| `dosage` | `Option<String>` | Free-form dosage string |
| `frequency` | `Option<String>` | e.g. "daily", "MWF", "as needed" |
| `started_at` | `NaiveDate` | Start date |
| `ended_at` | `Option<NaiveDate>` | End date (null = ongoing) |
| `notes` | `Option<String>` | Free-text |
| `target_biomarkers` | `Vec<i64>` | FKs → Biomarkers this intervention is expected to influence |

**Intervention–Biomarker Link Table:** `intervention_biomarker_targets`

| Field | Type |
|---|---|
| `intervention_id` | `i64` FK |
| `biomarker_id` | `i64` FK |
| `expected_effect` | `String` — `increase`, `decrease`, `stabilize` |

---

## 3. Architecture

### 3.1 High-Level Stack

```
┌──────────────────────────────────────────────┐
│                  Browser UI                   │
│            (HTML/CSS/JS + HTMX)              │
└──────────────┬───────────────────────────────┘
               │ HTTP (localhost)
┌──────────────▼───────────────────────────────┐
│              Axum Web Server                  │
│  ┌─────────┐ ┌──────────┐ ┌───────────────┐  │
│  │  REST   │ │  Upload  │ │  Extraction   │  │
│  │  API    │ │  Handler │ │  Endpoint     │  │
│  └────┬────┘ └────┬─────┘ └───────┬───────┘  │
│       │           │               │           │
│  ┌────▼───────────▼───────────────▼────────┐  │
│  │           Service Layer                 │  │
│  │  ┌──────────┐ ┌────────┐ ┌───────────┐  │  │
│  │  │Biomarker │ │Report  │ │Interven-  │  │  │
│  │  │Service   │ │Service │ │tion Svc   │  │  │
│  │  └──────────┘ └───┬────┘ └───────────┘  │  │
│  └────────────────────┼────────────────────┘  │
│                       │                       │
│  ┌────────────────────▼────────────────────┐  │
│  │        Rig Agent Orchestration          │  │
│  │                                         │  │
│  │  ┌─────────────────────────────────┐    │  │
│  │  │  Extraction Agent               │    │  │
│  │  │  (preamble + tools + extractor) │    │  │
│  │  └──────────────┬──────────────────┘    │  │
│  │                 │ tool calls             │  │
│  │  ┌──────────────▼──────────────────┐    │  │
│  │  │  Tool Server                    │    │  │
│  │  │  ┌───────────┐ ┌────────────┐   │    │  │
│  │  │  │LoincLookup│ │UnitConvert │   │    │  │
│  │  │  └───────────┘ └────────────┘   │    │  │
│  │  │  ┌───────────┐ ┌────────────┐   │    │  │
│  │  │  │Validate   │ │ThinkTool   │   │    │  │
│  │  │  └───────────┘ └────────────┘   │    │  │
│  │  └─────────────────────────────────┘    │  │
│  └─────────────────────────────────────────┘  │
│                       │                       │
│  ┌────────────────────▼────────────────────┐  │
│  │         SQLite (via sqlx)               │  │
│  │         data/hermes.db                  │  │
│  └─────────────────────────────────────────┘  │
└───────────────────────────────────────────────┘
               │
               │ HTTP (localhost:11434)
┌──────────────▼───────────────────────────────┐
│           Ollama (external process)           │
│        Model: qwen3.5:27b (configurable)     │
└──────────────────────────────────────────────┘
```

### 3.2 Crate / Module Layout

```
hermes-health/
├── Cargo.toml
├── migrations/           # sqlx migrations
│   ├── 001_init.sql
│   └── 002_interventions.sql
├── data/
│   ├── loinc_subset.csv  # Curated LOINC catalog (embedded at build time)
│   └── reports/          # Stored uploaded files (gitignored)
├── src/
│   ├── main.rs
│   ├── config.rs         # App configuration (CLI args, env, TOML)
│   ├── db/
│   │   ├── mod.rs
│   │   ├── models.rs     # SQLx structs (FromRow)
│   │   ├── queries.rs    # Named query functions
│   │   └── migrate.rs    # Embedded migration runner
│   ├── api/
│   │   ├── mod.rs
│   │   ├── biomarkers.rs # CRUD + trend endpoints
│   │   ├── observations.rs
│   │   ├── reports.rs    # Upload + extraction status
│   │   ├── interventions.rs
│   │   └── export.rs     # FHIR R4 JSON export
│   ├── services/
│   │   ├── mod.rs
│   │   ├── biomarker.rs  # Business logic, calculated markers
│   │   ├── report.rs     # Upload processing, orchestrates extraction pipeline
│   │   ├── intervention.rs
│   │   └── loinc.rs      # LOINC lookup, alias matching
│   ├── agent/
│   │   ├── mod.rs            # Agent construction, Rig client setup
│   │   ├── extraction.rs     # Extraction agent: preamble, tool wiring, multi-turn loop
│   │   ├── tools/
│   │   │   ├── mod.rs
│   │   │   ├── loinc_lookup.rs   # Tool: resolve marker name → LOINC code candidates
│   │   │   ├── unit_convert.rs   # Tool: check/convert unit for a given LOINC code
│   │   │   ├── validate.rs       # Tool: validate extracted row (range, type, completeness)
│   │   │   └── submit_results.rs # Tool: submit final extraction batch for review
│   │   └── prompts.rs       # System preamble templates
│   ├── ingest/
│   │   ├── mod.rs
│   │   ├── pdf.rs        # PDF text extraction (pre-Ollama)
│   │   ├── csv.rs        # CSV parsing & column mapping
│   │   ├── normalize.rs  # Unit conversion, precision handling, LOINC matching
│   │   └── units.rs      # Conversion registry, unit alias table, UCUM normalization
│   ├── export/
│   │   ├── mod.rs
│   │   └── fhir.rs       # FHIR R4 Bundle/Observation serialization
│   └── ui/
│       └── static/       # Embedded frontend assets
└── tests/
    ├── integration/
    └── fixtures/         # Sample lab PDFs and CSVs for testing
```

### 3.3 Key Crate Dependencies

| Crate | Purpose |
|---|---|
| `axum` | HTTP server, routing, extractors |
| `sqlx` (SQLite feature) | Async database, compile-time checked queries, migrations |
| `serde` / `serde_json` | Serialization for API, config, FHIR export |
| `schemars` (v1) | JSON Schema derivation for Rig extractor/tool args |
| `rig-core` (Ollama feature) | LLM agent framework — Ollama provider, tool trait, extractor, prompt chaining |
| `tokio` | Async runtime |
| `pdf-extract` or `lopdf` | PDF text extraction (pre-LLM step) |
| `csv` | CSV parsing |
| `chrono` | Date/time handling |
| `rust-embed` | Embed static UI assets + LOINC subset into binary |
| `clap` | CLI argument parsing |
| `tracing` / `tracing-subscriber` | Structured logging (also used by Rig's telemetry) |
| `sha2` | File hashing for deduplication |
| `strsim` | Fuzzy string matching (Jaro-Winkler) for LOINC alias resolution |
| `tower-http` | CORS, compression, static file serving middleware |
| `thiserror` | Ergonomic error types for tool implementations |

---

## 4. Core Workflows

### 4.1 Manual Observation Entry

```
POST /api/observations
{
  "biomarker": "2093-3",       // LOINC code or alias
  "value": 185.0,
  "unit": "mg/dL",
  "observed_at": "2026-03-15",
  "fasting": true,
  "notes": "post 5-day fast"
}
```

The service layer resolves the biomarker (by LOINC code or alias), validates the unit (converting if needed), stores the observation, and triggers recalculation of any dependent calculated markers (e.g. if LDL or HDL changes, recalculate TG/HDL ratio).

### 4.2 Batch Entry

```
POST /api/observations/batch
[
  { "biomarker": "Total Cholesterol", "value": 185, ... },
  { "biomarker": "HDL-C", "value": 52, ... },
  { "biomarker": "Triglycerides", "value": 110, ... }
]
```

Alias resolution allows natural names from lab reports. Failed resolutions return a 422 with suggestions.

### 4.3 Lab Report Upload & Extraction

This is the most complex workflow and the primary differentiator.

```
Step 1: Upload
  POST /api/reports/upload (multipart/form-data)
  → Store file in data/reports/
  → Compute SHA-256, reject duplicates
  → Create Report record (status: pending)
  → Return report_id

Step 2: Extract (async background task — Rig agent pipeline)
  → If PDF: extract raw text via pdf-extract
  → If CSV: read raw content
  → Feed raw text to the Extraction Agent (see §4.4)
  → Agent uses tools to resolve LOINC codes, validate units, and self-correct
  → Agent submits structured results via the SubmitResults tool
  → Store raw + resolved extraction in Report.raw_extraction
  → Update status: extracted

Step 3: Review (human-in-the-loop)
  GET /api/reports/{id}/extraction
  → Returns extracted observations with LOINC match confidence
  → Shows unresolved markers the agent flagged for manual mapping
  → User confirms, corrects, or rejects each row

Step 4: Commit
  POST /api/reports/{id}/commit
  → Accepted observations are written to the observations table
  → Any user corrections to marker names are persisted as new LOINC aliases
  → Status: reviewed
```

### 4.4 Agentic Extraction Pipeline (Rig)

The extraction pipeline uses [Rig](https://rig.rs), a Rust-native LLM agent framework, to orchestrate a multi-turn, tool-augmented extraction from lab reports via a local Ollama model. This replaces a single-shot prompt with an agent that can reason about ambiguity, validate its own output, and call back into the application's domain logic.

#### Why Rig over direct HTTP calls

- **Tool use** — the agent can call LOINC lookup, unit conversion, and validation tools during extraction, rather than relying on the LLM's internal knowledge of medical coding.
- **Self-correction** — multi-turn: if validation fails (e.g. unrecognized unit, value outside plausible range), the agent can re-examine the raw text and try again.
- **Structured output** — Rig's `Extractor` and `JsonSchema`-derived types enforce output shape at the framework level, not just via prompt engineering.
- **Provider abstraction** — switching from Ollama to another provider (or a cloud model for difficult reports) requires only changing the client, not the tool/agent code.
- **ThinkTool** — Rig's built-in `ThinkTool` gives the model a dedicated space for structured reasoning before committing to an extraction, which helps with messy or ambiguous report layouts.

#### Extraction Agent Design

```rust
// Conceptual — not compilable, illustrates architecture

use rig::providers::ollama;
use rig::tool::server::{ToolServer, ToolServerHandle};
use rig::tools::think::ThinkTool;
use rig::completion::Prompt;

// 1. Create Ollama client
let client = ollama::Client::from_url("http://localhost:11434");

// 2. Set up shared tool server
let tool_server: ToolServerHandle = ToolServer::new()
    .tool(LoincLookupTool::new(loinc_catalog.clone()))
    .tool(UnitConvertTool::new(unit_registry.clone()))
    .tool(ValidateRowTool::new(biomarker_repo.clone()))
    .tool(SubmitResultsTool::new(extraction_sink.clone()))
    .tool(ThinkTool)
    .run();

// 3. Build extraction agent
let extraction_agent = client
    .agent("qwen3.5:27b")
    .preamble(EXTRACTION_PREAMBLE)
    .tool_server(tool_server)
    .temperature(0.0)
    .build();

// 4. Run extraction
let result = extraction_agent
    .prompt(&format!("Extract all biomarker results:\n\n{raw_text}"))
    .await?;
```

#### Custom Tools

Each tool implements Rig's `Tool` trait. The agent decides when and how to call them.

**`LoincLookupTool`** — resolves a marker name to LOINC code candidates.

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
struct LoincLookupArgs {
    /// The biomarker name exactly as printed on the lab report
    marker_name: String,
}

#[derive(Serialize)]
struct LoincCandidate {
    loinc_code: String,
    canonical_name: String,
    confidence: f64,     // 0.0–1.0 (1.0 = exact match)
}
```

The tool performs exact match → alias match → fuzzy match (Jaro-Winkler ≥ 0.85) against the embedded LOINC catalog. Returns up to 3 candidates ranked by confidence. If no match ≥ 0.85, returns an empty list and the agent can flag the row as `unmatched`.

**`UnitConvertTool`** — validates and converts a unit for a specific biomarker.

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
struct UnitConvertArgs {
    /// LOINC code of the biomarker
    loinc_code: String,
    /// The value as reported
    value: f64,
    /// The unit as reported on the lab report
    from_unit: String,
}

#[derive(Serialize)]
struct ConversionResult {
    canonical_unit: String,
    canonical_value: f64,
    conversion_applied: bool,
    precision: u8,
}
```

Looks up the biomarker's canonical unit and the conversion registry (§8). Returns the converted value with precision preserved, or an error if the unit is unrecognized.

**`ValidateRowTool`** — sanity-checks an extracted observation.

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
struct ValidateArgs {
    /// LOINC code
    loinc_code: String,
    /// Numeric value in canonical units
    value: f64,
    /// Reference range low (if extracted)
    reference_low: Option<f64>,
    /// Reference range high (if extracted)
    reference_high: Option<f64>,
}

#[derive(Serialize)]
struct ValidationResult {
    valid: bool,
    warnings: Vec<String>,  // e.g. "value 5000 mg/dL is implausibly high for Total Cholesterol"
}
```

Checks: value within plausible physiological range (configurable per biomarker), reference range sanity (low < high, within known bounds), and flags detection-limit markers (`<` / `>`).

**`SubmitResultsTool`** — the agent calls this when extraction is complete.

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
struct SubmitResultsArgs {
    /// The extracted observations ready for human review
    observations: Vec<ExtractedObservation>,
    /// Markers the agent could not confidently resolve
    unresolved: Vec<UnresolvedMarker>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct ExtractedObservation {
    marker_name: String,
    loinc_code: String,
    value: f64,
    original_value: String,
    unit: String,
    canonical_unit: String,
    canonical_value: f64,
    reference_low: Option<f64>,
    reference_high: Option<f64>,
    flag: Option<String>,        // "H", "L", or null
    confidence: f64,             // LOINC match confidence
    detection_limit: Option<String>,
}
```

#### Agent Preamble

```
You are a clinical lab report extraction agent. Your task is to extract
every biomarker result from the provided lab report text.

For EACH result you find:
1. Use the loinc_lookup tool to resolve the marker name to a LOINC code.
   If multiple candidates are returned, pick the highest-confidence match.
   If no match is found, include it in the unresolved list.
2. Use the unit_convert tool to normalize the value to canonical units.
3. Use the validate_row tool to sanity-check the value.
4. If validation warns about an implausible value, re-examine the raw text
   — you may have misread a decimal point or unit.

Use the think tool when you encounter ambiguous layouts, merged columns,
or unclear marker names. Reason through the structure before extracting.

When you have processed ALL markers, call submit_results with the full
batch. Do not submit partial results.
```

#### Multi-Turn Extraction Flow

```
Turn 1: Agent receives raw text
  → Thinks about report structure (ThinkTool)
  → Identifies markers in the text

Turn 2–N: For each marker (or batch of markers)
  → Calls LoincLookupTool
  → Calls UnitConvertTool
  → Calls ValidateRowTool
  → If validation fails → re-examines text, tries alternative parsing

Final Turn:
  → Calls SubmitResultsTool with complete extraction
  → Agent response indicates completion
```

The agent runs as a Tokio background task. Progress is tracked via the Report's `extraction_status` field, which the frontend can poll.

#### Ollama Configuration

| Setting | Default | Notes |
|---|---|---|
| `ollama_url` | `http://localhost:11434` | Configurable via `config.toml` |
| `model` | `qwen3.5:27b` | Must support tool calling; configurable via `config.toml` |
| `temperature` | `0.0` | Deterministic extraction |
| `timeout` | `300s` | Longer than single-shot — agent runs multiple turns |

**Model requirements:** The Ollama model must support function/tool calling. Not all models do — Gemma, for instance, does not. The default is `qwen3.5:27b`, which provides strong structured extraction and tool calling. Alternative models to test if switching: `llama3:8b-instruct`, `qwen2.5:7b-instruct`, `mistral:7b-instruct`. The model is configurable via `config.toml` without recompilation. The agent's effectiveness depends heavily on model choice; benchmarking with real Singapore lab report formats (Parkway, Raffles Medical, Quest) is recommended.

#### Fallback: Direct Extraction Mode

For models that don't support tool calling, or as a simpler alternative for clean CSV imports, the pipeline falls back to Rig's `Extractor` — a single-shot structured extraction that uses the `JsonSchema`-derived type to enforce output shape:

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
struct LabResultRow {
    marker_name: String,
    value: f64,
    unit: String,
    reference_low: Option<f64>,
    reference_high: Option<f64>,
    flag: Option<String>,
}

let extractor = client
    .extractor::<Vec<LabResultRow>>("qwen3.5:27b")
    .preamble("Extract all biomarker results from the lab report.")
    .build();

let rows = extractor.extract(&raw_text).await?;
// → Post-process through LOINC matching + unit conversion (non-agentic)
```

This is essentially what the spec described before, but now with Rig's type-safe extraction instead of raw JSON parsing. LOINC matching and unit conversion happen in application code rather than via tool calls.

---

## 5. API Surface

All endpoints are prefixed with `/api/v1`.

### Biomarkers

| Method | Path | Description |
|---|---|---|
| `GET` | `/biomarkers` | List all (filterable by category) |
| `GET` | `/biomarkers/{id}` | Detail with observation history |
| `GET` | `/biomarkers/{id}/trend` | Time-series data + statistical trend analysis |
| `POST` | `/biomarkers` | Create custom biomarker |
| `PUT` | `/biomarkers/{id}` | Update ranges, aliases |
| `GET` | `/biomarkers/out-of-range` | All markers with latest value outside reference or optimal range |
| `GET` | `/biomarkers/dashboard` | Summary: counts by status (in-range, out-of-range, stale, trending-worse) |

### 5.1 Trend Analysis Detail

The `/biomarkers/{id}/trend` endpoint returns both raw time-series data and computed trend statistics.

**Query parameters:**

| Param | Default | Description |
|---|---|---|
| `window_days` | 365 | Look-back window for trend calculation |
| `min_points` | 3 | Minimum observations required to compute a trend |

**Response includes:**

```json
{
  "biomarker_id": 42,
  "loinc_code": "2093-3",
  "observations": [ /* time-series array */ ],
  "trend": {
    "direction": "decreasing",
    "slope": -2.3,
    "slope_unit": "mg/dL per year",
    "r_squared": 0.87,
    "rate_of_change_pct": -4.1,
    "latest_value": 178.0,
    "previous_value": 185.0,
    "status": "improving",
    "alert": null
  }
}
```

**Trend computation (in `services/biomarker.rs`):**

- **Linear regression** (ordinary least squares) on observations within the window, with time as the independent variable (days from first observation).
- **Direction**: `increasing`, `decreasing`, or `stable` (slope within ±1% of mean per year).
- **Status**: contextual interpretation using reference/optimal ranges: `improving` (trending toward optimal), `worsening` (trending away), `stable`, or `insufficient_data`.
- **Rate of change**: percentage change between the two most recent observations, and annualized rate from the regression slope.
- **Alerts** (surfaced on dashboard):
  - `approaching_limit` — projected to cross reference range boundary within 6 months at current rate
  - `rapid_change` — rate of change exceeds configurable threshold (default: >20% annualized)
  - `reversal` — trend direction has flipped compared to the previous window

### Observations

| Method | Path | Description |
|---|---|---|
| `GET` | `/observations` | List (filterable by biomarker, date range) |
| `POST` | `/observations` | Single entry |
| `POST` | `/observations/batch` | Batch entry |
| `DELETE` | `/observations/{id}` | Soft delete |

### Reports

| Method | Path | Description |
|---|---|---|
| `POST` | `/reports/upload` | Upload PDF or CSV |
| `GET` | `/reports` | List all reports |
| `GET` | `/reports/{id}` | Report detail + extraction status |
| `GET` | `/reports/{id}/extraction` | Extracted data for review |
| `POST` | `/reports/{id}/commit` | Commit reviewed extractions |

### Interventions

| Method | Path | Description |
|---|---|---|
| `GET` | `/interventions` | List all (filterable: active, category) |
| `POST` | `/interventions` | Create |
| `PUT` | `/interventions/{id}` | Update (e.g. end date, dosage change) |
| `DELETE` | `/interventions/{id}` | Soft delete |
| `GET` | `/interventions/{id}/impact` | Biomarker trends overlaid with intervention timeline |

### Export

| Method | Path | Description |
|---|---|---|
| `GET` | `/export/fhir` | Full FHIR R4 Bundle (all observations) |
| `GET` | `/export/fhir?from=&to=` | Date-filtered export |
| `GET` | `/export/csv` | Flat CSV export |

---

## 6. Frontend

### 6.1 Approach

Server-rendered HTML templates (Askama or Minijinja) + HTMX for interactivity. Keeps the "single binary" promise clean — no separate JS build step, no Node dependency. Charts via a lightweight library like uPlot (small footprint, touch-friendly) loaded from embedded static assets.

### 6.2 Responsive, Mobile-First Design

All views are designed mobile-first and scale up to desktop. The app should be fully usable on a phone browser (e.g. checking results while at the lab or on the go).

**Layout strategy:**

- CSS: minimal custom stylesheet + a lightweight utility framework (e.g. PicoCSS or hand-rolled variables). No Tailwind build step.
- Breakpoints: single-column on mobile (< 640px), two-column on tablet (640–1024px), full layout on desktop (> 1024px).
- Charts: uPlot supports touch events natively. On mobile, charts render full-width with pinch-to-zoom and tap-for-tooltip.
- Tables: horizontal scroll with sticky first column on mobile. The review table (lab import) uses a card-based layout on narrow screens instead of a table.
- Navigation: hamburger menu on mobile, sidebar on desktop.
- PWA-ready: include a `manifest.json` and service worker stub so the app can be added to mobile home screen. Offline support is not a v1 goal, but the manifest gives a native-app feel.

### 6.3 Key Views

1. **Dashboard** — at-a-glance summary: markers out of range (red/amber/green), latest values vs. optimal, trend direction indicators (↑↓→), active interventions, days since last lab. On mobile: vertically stacked cards. On desktop: grid layout.

2. **Biomarker Detail** — time-series chart with reference/optimal bands, overlaid intervention start/end markers, individual data point tooltips (lab name, fasting status, notes). Below the chart: trend statistics panel (direction, slope, rate-of-change, alerts).

3. **Lab Report Import** — drag-and-drop upload (file picker on mobile) → extraction progress indicator → review table with editable fields, LOINC match indicators, confirm/reject per row. Card layout on mobile.

4. **Interventions Timeline** — Gantt-style view of all interventions, color-coded by category, with biomarker trends layered underneath. On mobile: vertical timeline (stacked) instead of horizontal Gantt.

5. **Data Entry** — form for manual observation entry with biomarker autocomplete (searches LOINC name + aliases). Large touch targets on mobile.

6. **Settings** — Ollama configuration, LOINC catalog management, reference/optimal range overrides, export options.

---

## 7. LOINC Integration

### 7.1 Catalog Subset

The full LOINC database has ~100K codes. Hermes ships with a curated subset (~500 codes) covering common biomarkers:

- **Lipid Panel:** Total Cholesterol (2093-3), LDL-C (13457-7), HDL-C (2085-9), Triglycerides (2571-8), ApoB (1884-6), Lp(a) (10835-7)
- **Metabolic:** Glucose (2345-7), HbA1c (4548-4), Insulin (2484-4), HOMA-IR (calculated)
- **Liver:** ALT (1742-6), AST (1920-8), GGT (2324-2), ALP (6768-6), Bilirubin (1975-2)
- **Kidney:** Creatinine (2160-0), BUN (3094-0), eGFR (calculated), Cystatin C (33863-2)
- **Thyroid:** TSH (3016-3), Free T4 (3024-7), Free T3 (3051-0)
- **Inflammatory:** hsCRP (30522-7), ESR (4537-7), Ferritin (2276-4), Homocysteine (13965-9)
- **Hormonal:** Testosterone (2986-8), DHEA-S (2191-5), Cortisol (2143-6), IGF-1 (2484-4)
- **Hematology:** CBC panel codes
- **Vitamins/Minerals:** Vitamin D (1989-3), B12 (2132-9), Folate (2284-8), Magnesium (19123-9), Zinc (2601-3)
- **Longevity-specific:** SHBG, Insulin-like growth factors, oxidized LDL

### 7.2 Alias System

Lab reports use inconsistent naming. The alias system maps variations to canonical LOINC codes:

```
"Total Cholesterol" → 2093-3
"TC"                → 2093-3
"Cholesterol, Total"→ 2093-3
"CHOL"              → 2093-3
```

New aliases are learned from user corrections during the report review step and persisted for future imports.

### 7.3 Calculated Markers

Some markers are derived from others:

| Marker | Formula | Inputs |
|---|---|---|
| TG/HDL Ratio | `triglycerides / hdl` | 2571-8, 2085-9 |
| LDL (Friedewald) | `tc - hdl - (tg / 5)` | 2093-3, 2085-9, 2571-8 |
| HOMA-IR | `(glucose × insulin) / 405` | 2345-7, 2484-4 |
| eGFR (CKD-EPI) | CKD-EPI 2021 equation | 2160-0, age, sex |
| ApoB/ApoA1 Ratio | `apob / apoa1` | 1884-6, 1869-7 |

Calculated markers auto-update when their inputs change.

---

## 8. Unit Conversion & Precision

### 8.1 Conversion Registry

Each biomarker defines a **canonical unit** (stored in the `biomarkers.unit` field). A conversion registry maps known alternative units to conversion factors, keyed by LOINC code. This is biomarker-specific because conversion factors depend on molecular weight.

**Schema: `unit_conversions` table**

| Field | Type | Description |
|---|---|---|
| `id` | `i64` | Internal PK |
| `biomarker_id` | `i64` | FK → Biomarker |
| `from_unit` | `String` | Source unit (UCUM code, e.g. `mmol/L`) |
| `to_unit` | `String` | Target unit (always the biomarker's canonical unit) |
| `factor` | `f64` | Multiply source value by this to get canonical value |
| `offset` | `f64` | Additive offset (default 0.0; needed for e.g. °F → °C) |

**Formula:** `canonical_value = (original_value × factor) + offset`

**Seed data — common Singapore lab conversions:**

| Biomarker | From | To (canonical) | Factor | Notes |
|---|---|---|---|---|
| Total Cholesterol (2093-3) | `mmol/L` | `mg/dL` | 38.67 | MW cholesterol |
| LDL-C (13457-7) | `mmol/L` | `mg/dL` | 38.67 | MW cholesterol |
| HDL-C (2085-9) | `mmol/L` | `mg/dL` | 38.67 | MW cholesterol |
| Triglycerides (2571-8) | `mmol/L` | `mg/dL` | 88.57 | MW triglycerides (avg) |
| Glucose (2345-7) | `mmol/L` | `mg/dL` | 18.018 | MW glucose |
| Creatinine (2160-0) | `µmol/L` | `mg/dL` | 0.01131 | MW creatinine |
| Testosterone (2986-8) | `nmol/L` | `ng/dL` | 28.84 | MW testosterone |
| Vitamin D (1989-3) | `nmol/L` | `ng/mL` | 0.4006 | MW 25-OH-D |
| Vitamin B12 (2132-9) | `pmol/L` | `pg/mL` | 1.355 | MW cobalamin |
| Homocysteine (13965-9) | `µmol/L` | `µmol/L` | 1.0 | SI is canonical |
| hsCRP (30522-7) | `nmol/L` | `mg/L` | 0.105 | MW CRP monomer |
| HbA1c (4548-4) | `mmol/mol` | `%` | 0.0915 (offset: 2.15) | IFCC → NGSP: `% = 0.0915 × mmol/mol + 2.15` |

**Conversion rules:**

- If `original_unit == canonical_unit` → no conversion, store as-is.
- If `original_unit` has a registered conversion → apply automatically at write time.
- If `original_unit` is unrecognized → reject the observation with an error, surface to the user during import review. The user can then either add a conversion rule or correct the unit.
- Unit string matching is case-insensitive and alias-aware (e.g. `"umol/L"` = `"µmol/L"`, `"mg/dl"` = `"mg/dL"`).
- Users can add custom conversions via the API or settings UI.

**Unit alias table (for string normalization):**

| Input variants | Normalized UCUM |
|---|---|
| `mg/dl`, `mg/DL`, `MG/DL` | `mg/dL` |
| `umol/L`, `umol/l`, `μmol/L` | `µmol/L` |
| `nmol/l`, `NMOL/L` | `nmol/L` |
| `pg/ml`, `PG/ML` | `pg/mL` |
| `%`, `percent` | `%` |
| `g/L`, `g/l` | `g/L` |
| `IU/L`, `iu/l`, `U/L` | `U/L` |

### 8.2 Precision Handling

Lab instruments have finite precision. A result of `"5.2"` (1 decimal place) and `"5.20"` (2 decimal places) carry different precision semantics. The system preserves this throughout the data lifecycle.

**Storage strategy — dual representation:**

Every observation stores both the original string and the normalized numeric value (see §2.2 updated fields: `original_value`, `value`, `precision`).

**Precision derivation rules:**

| `original_value` string | `precision` | `value` (f64) |
|---|---|---|
| `"185"` | 0 | 185.0 |
| `"5.2"` | 1 | 5.2 |
| `"5.20"` | 2 | 5.2 |
| `"0.85"` | 2 | 0.85 |
| `"<0.5"` | 1 (with flag) | 0.5 (with `notes`: "below detection limit") |
| `">200"` | 0 (with flag) | 200.0 (with `notes`: "above measurement range") |

**Precision through conversion:**

When a unit conversion is applied, the result should not claim more precision than the source measurement. The rule: **converted value is rounded to the same number of significant figures as the original.**

Example:
- Lab reports Total Cholesterol as `"4.8"` mmol/L (2 sig figs)
- Conversion: 4.8 × 38.67 = 185.616
- Rounded to 2 sig figs: `186` mg/dL → stored as `value = 186.0`, `precision = 0`
- `original_value = "4.8"`, `original_unit = "mmol/L"` preserved for audit

**Display formatting:**

The UI uses `precision` to format values: `format!("{:.prec$}", value, prec = precision)`. This prevents false precision artifacts like `"185.00000"` from `f64` representation.

**Detection limit values (`<` and `>` prefixes):**

Lab reports sometimes contain values like `"<0.5"` or `">200"` indicating the result is beyond the instrument's measurement range. These are stored as:
- `value` = the boundary number (0.5 or 200)
- `original_value` = the full string including the prefix
- `notes` automatically appended with `"below detection limit"` or `"above measurement range"`
- The `detection_limit` field (`Option<String>`: `"<"`, `">"`, or `null`) on the Observation model allows queries to exclude or highlight these values in trend analysis

### 8.3 Module Placement

Unit conversion and precision logic lives in `src/ingest/normalize.rs` and is called by both the manual entry path (API) and the report extraction pipeline:

```
Input (API or Ollama extraction)
  → parse original_value string → derive precision
  → normalize unit string (alias lookup)
  → lookup conversion factor (registry)
  → apply conversion with precision preservation
  → return NormalizedObservation { value, original_value, original_unit, precision }
```

---

## 9. Data Export

### 9.1 FHIR R4

Export produces a FHIR R4 `Bundle` of `Observation` resources, compatible with any FHIR-compliant health record system.

```json
{
  "resourceType": "Bundle",
  "type": "collection",
  "entry": [
    {
      "resource": {
        "resourceType": "Observation",
        "status": "final",
        "code": {
          "coding": [{
            "system": "http://loinc.org",
            "code": "2093-3",
            "display": "Total Cholesterol"
          }]
        },
        "valueQuantity": {
          "value": 185.0,
          "unit": "mg/dL",
          "system": "http://unitsofmeasure.org",
          "code": "mg/dL"
        },
        "effectiveDateTime": "2026-03-15",
        "referenceRange": [{
          "low": { "value": 125, "unit": "mg/dL" },
          "high": { "value": 200, "unit": "mg/dL" }
        }]
      }
    }
  ]
}
```

### 9.2 CSV

Flat export: `date, loinc_code, marker_name, value, unit, reference_low, reference_high, flag, lab, fasting, notes`

---

## 10. Configuration

All configuration via `config.toml` with CLI overrides and environment variable fallbacks.

```toml
[server]
host = "127.0.0.1"
port = 8080

[database]
path = "data/hermes.db"

[ollama]
url = "http://localhost:11434"
model = "qwen3.5:27b"             # Must support tool calling; configurable
temperature = 0.0
timeout_seconds = 300           # Agent runs multiple turns

[extraction]
mode = "agentic"               # "agentic" (tool-augmented) or "direct" (single-shot extractor)
max_agent_turns = 20           # Safety limit on agent loop iterations
validation_strictness = "warn" # "warn" = flag implausible values, "strict" = reject them

[user]
date_of_birth = "1985-01-01"   # For eGFR and age-dependent ranges
sex = "male"                    # For sex-dependent reference ranges

[display]
date_format = "%Y-%m-%d"
default_trend_window_days = 365

[trends]
min_data_points = 3                # Minimum observations to compute a trend
rapid_change_threshold_pct = 20.0  # Annualized % change that triggers a rapid_change alert
projection_horizon_days = 180      # How far ahead to project for approaching_limit alerts
```

---

## 11. Development Phases

### Phase 1 — Foundation
- SQLite schema + migrations
- Biomarker CRUD with LOINC subset
- Observation entry (single + batch)
- Trend query: time-series JSON + statistical trend detection (linear regression, rate-of-change, direction)
- CSV export
- CLI-only interface (no web UI yet)

### Phase 2 — Web UI + Charting
- Axum server with embedded static assets
- Responsive, mobile-first layout (all views usable on phone)
- Dashboard, biomarker detail, data entry views
- Time-series charts with reference/optimal bands + trend indicators
- Out-of-range alerting view
- Trend alerts: visual indicators for worsening/improving markers

### Phase 3 — Lab Report Ingestion
- PDF text extraction
- CSV column detection and mapping
- Rig agent setup: Ollama provider, tool server, extraction agent
- Custom tools: LoincLookup, UnitConvert, ValidateRow, SubmitResults
- Agentic extraction pipeline with multi-turn self-correction
- Fallback: Rig Extractor for direct (non-agentic) extraction
- Human-in-the-loop review UI
- Alias learning from corrections

### Phase 4 — Interventions
- Intervention CRUD
- Intervention ↔ biomarker linking
- Timeline visualization
- Trend overlay (biomarker chart with intervention markers)

### Phase 5 — Export & Polish
- FHIR R4 export
- Backup/restore commands
- PWA manifest for mobile home screen install (optional)

### Phase 6 — Advanced Import
- Vision-based PDF extraction for reports where values are rendered as images/graphics (e.g., web portal print-to-PDF with colored bar charts instead of text values)
- Extract images from PDF pages, send to a vision-capable LLM (e.g., LLaVA via Ollama) to read values from bar charts and visual indicators
- OCR fallback via tesseract for scanned/image-only PDFs
- Configurable reference range sources (longevity-oriented ranges vs conventional lab ranges, user-editable)

---

## 12. Open Questions

All resolved.

1. ~~**Frontend framework decision**~~ — **RESOLVED:** HTMX + server-rendered templates (Askama/Minijinja) for v1. Keeps single-binary story clean, no JS build step.

2. ~~**Ollama model selection**~~ — **RESOLVED:** Default to `qwen3.5:27b`. Configurable via `config.toml` `[ollama].model`. Must support tool calling. Benchmark against real Singapore lab report formats (Parkway, Raffles Medical, Quest) and adjust if needed.

3. ~~**Unit conversion scope**~~ — **RESOLVED:** See §8. Conversion registry seeded with common Singapore SI ↔ conventional conversions. User-extensible. Unrecognized units are rejected and surfaced for manual resolution.

4. ~~**Multi-user**~~ — **RESOLVED:** Single-user, no auth. Simplifies every layer.

5. ~~**Trend analysis**~~ — **RESOLVED:** v1 includes statistical trend detection. See §5.1 (Biomarker trend endpoint). Linear regression on configurable window, rate-of-change alerts, direction classification.

6. ~~**Mobile access**~~ — **RESOLVED:** Responsive design from v1. Mobile-first CSS with HTMX. See §6.2.
