# Hermes Health — Implementation Handoff

This directory contains the full specification and visual wireframes for Hermes Health, a Rust-based biomarker tracking application.

## Contents

```
hermes-handoff/
├── README.md              ← You are here
├── SPEC.md                ← Full application specification (v0.2.0-draft)
└── wireframes/
    ├── 01-dashboard-desktop.html
    ├── 02-dashboard-mobile.html
    ├── 03-biomarker-detail.html
    ├── 04-lab-import-review.html
    └── 05-data-entry-interventions.html
```

## How to use this

### The spec

`SPEC.md` is the single source of truth. It covers:

- **§1** Vision and design principles
- **§2** Domain model (Biomarkers, Observations, Reports, Interventions) with full field-level schemas
- **§3** Architecture: Axum + SQLite + Rig agent layer, module layout, crate dependencies
- **§4** Core workflows including the agentic extraction pipeline (Rig + Ollama)
- **§5** REST API surface with trend analysis detail
- **§6** Frontend approach (HTMX, mobile-first responsive)
- **§7** LOINC integration (catalog subset, alias system, calculated markers)
- **§8** Unit conversion registry and precision handling
- **§9** FHIR R4 and CSV export formats
- **§10** Configuration (`config.toml` structure)
- **§11** Phased development plan
- **§12** Resolved decisions

### The wireframes

The wireframes are standalone HTML files. Open them in a browser to see the visual reference. They use no external dependencies — just inline CSS with a design system consistent across all views.

| File | View | Layout | Spec reference |
|------|------|--------|----------------|
| `01-dashboard-desktop.html` | Dashboard | Desktop (1024px+) | §6.3 view #1 |
| `02-dashboard-mobile.html` | Dashboard | Mobile (<640px) | §6.2 + §6.3 view #1 |
| `03-biomarker-detail.html` | Biomarker detail | Desktop | §5.1 + §6.3 view #2 |
| `04-lab-import-review.html` | Lab import review | Desktop | §4.3, §4.4, §6.3 view #3 |
| `05-data-entry-interventions.html` | Data entry + interventions | Mobile | §2.4, §6.3 views #4 & #5 |

Each wireframe contains a yellow annotation bar at the top identifying what it is and which spec sections it maps to.

### Design patterns visible in the wireframes

- **Color coding**: Red = out of range / worsening. Amber = approaching limit / below optimal. Green = in range / improving. Blue = data points / info. Purple = intervention markers.
- **Trend indicators**: ▼ decreasing, ▲ increasing, ▶ stable. Combined with status text (worsening/improving/stable) and rate-of-change percentage.
- **LOINC confidence badges**: Green "exact"/"alias"/"fuzzy" for matched markers, amber "low" for unresolved. Confidence score shown numerically.
- **Unit conversion annotations**: Rows where a unit conversion will be applied show the canonical value as a blue `→92 mg/dL` indicator in the action column.
- **Mobile adaptations**: Summary cards → 2×2 grid. Attention list → left-border-colored cards. Interventions → pill tags. Tables → card stacks. Nav → hamburger menu.

### Implementation order

Follow the phases in §11:

1. **Foundation** — SQLite schema, biomarker/observation CRUD, trend computation, CSV export, CLI only
2. **Web UI + Charting** — Axum server, HTMX templates, responsive layout, charts (uPlot), trend indicators
3. **Lab Report Ingestion** — PDF extraction, Rig agent with tools, human-in-the-loop review UI
4. **Interventions** — CRUD, biomarker linking, timeline visualization
5. **Export & Polish** — FHIR R4, backup/restore, PWA manifest

### Key decisions already made

- **Language**: Rust
- **Frontend**: HTMX + server-rendered templates (Askama or Minijinja)
- **Database**: SQLite via sqlx
- **LLM**: Ollama with Rig agent framework, default model `qwen3.5:27b`
- **Auth**: None (single-user)
- **Layout**: Mobile-first responsive
- **Trend analysis**: Statistical (linear regression, rate-of-change, alerts) from v1
- **Migration from Streamlit**: Not planned — clean start
