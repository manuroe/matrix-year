# AGENTS.md

[my] — Your year, on Matrix.

This document explains the internal architecture, data model, and extension points of **matrix-year (**`my`**)**. It is intended for humans *and* code-generation agents working on the project.

The goal is to avoid duplicating information across multiple meta-docs (CONTRIBUTING, ARCHITECTURE, etc.) while keeping the project easy to reason about.

---

## 1. Mental Model (Read This First)

```text
Matrix Homeserver
        ↓
     Crawler
        ↓
   Event Store (raw, append-only)
        ↓
   Stats Engine (derived data)
        ↓
     Renderers (html, gif, md, …)
```

Key principles:

- Crawling and rendering are **strictly separated**
- Raw data is **never mutated**, only appended
- Derived data can be recomputed at any time
- Rendering must be **pure** (no Matrix access)
- Everything is **local-first and privacy-respecting**

---

## 2. Crawl Phase

### Purpose

The crawler is responsible for fetching Matrix data and storing it locally in a resumable, incremental way.

It must:

- Be restartable at any time
- Never re-fetch data unnecessarily
- Never depend on renderers

### Inputs

- Matrix credentials (via config)
- Optional filters (rooms, date ranges)

### Outputs

- Raw Matrix events
- Crawl metadata (progress, cursors)

### Rules

- Crawling **must not compute stats**
- Crawling **must not transform message content**

---

## 3. Local Data Layout

### Root directory resolution

By default, `my` stores data **relative to the directory where the command is executed**.

Resolution order:

1. `--data <path>` if provided
2. `MY_DATA_DIR` environment variable
3. Current working directory (`./.my/`)

This makes the tool:

- Easy to run per-project / per-context
- Naturally sandboxed
- Friendly to temporary or shared machines

---

### Multi-account layout

The data directory is structured to support **multiple Matrix accounts** concurrently.

```text
.my/
├── accounts/
│   ├── @alice_example.org/
│   │   ├── meta/
│   │   ├── db.sqlite
│   │   └── renders/
│   └── @bob_example.com/
│       ├── meta/
│       ├── db.sqlite
│       └── renders/
│
└── global/
    └── version.json
```

Account identifiers must be filesystem-safe (e.g. `@alice_example.org`).

---

### SQLite storage

Each account has a single SQLite database:

```text
accounts/<account>/db.sqlite
```

Responsibilities:

- Store raw events
- Store crawl state (tokens, cursors)
- Store derived yearly stats

Render outputs remain **outside** the database.

---

## 4. Event Store

### Storage backend

The event store is implemented on top of **SQLite**.

Tables are append-only at the logical level, even if SQLite performs updates internally.

### Core tables (conceptual)

```sql
events(
  event_id TEXT PRIMARY KEY,
  room_id TEXT,
  sender TEXT,
  type TEXT,
  origin_ts INTEGER,
  json TEXT
)

rooms(
  room_id TEXT PRIMARY KEY,
  name TEXT,
  is_dm BOOLEAN
)

crawl_state(
  room_id TEXT PRIMARY KEY,
  since_token TEXT,
  last_ts INTEGER
)
```

### Guarantees

- Raw event JSON is stored verbatim
- Events are never modified once inserted
- Duplicate events must be ignored safely

---

## 5. Stats Engine

### Purpose

Convert raw events into **year-scoped derived statistics**.

### Properties

- Deterministic
- Idempotent
- Pure (no I/O beyond reading events, writing stats)

### Example Stats

```json
{
  "year": 2025,
  "messages": {
    "total": 4832,
    "by_month": { "01": 320, "02": 410 }
  },
  "rooms": {
    "top": ["!abc", "!def"]
  },
  "emoji": {
    "most_used": "😂"
  }
}
```

### Rules

- Stats **must not store message content**
- Stats **must not reference Matrix IDs unless needed**
- Stats **should be forward-compatible**

---

## 6. Render Phase

### Purpose

Renderers transform **stats (not events)** into human-facing outputs.

### Contract

Renderers:

- Take stats as input
- Produce files or serve content
- Never talk to Matrix
- Never mutate stats

### Supported / Planned Renderers

| Renderer | Description              |
| -------- | ------------------------ |
| `html`   | Static HTML recap  |
| `gif`    | Shareable animated recap |
| `md`     | Markdown report          |
| `json`   | Machine-readable stats   |

---

## 7. Renderer Interface (Conceptual)

```text
render(stats, options) -> output
```

Renderers may:

- Support themes
- Support minimal/full modes
- Choose their own layout

They must:

- Fail gracefully if stats are missing
- Be deterministic for the same input

---

## 8. Configuration

### Config discovery

Config is resolved in the following order:

1. `--config <path>`
2. `MY_CONFIG` environment variable
3. `./my.toml`
4. `~/.config/my/config.toml`

---

### Multi-account config

```toml
[[account]]
name = "alice"
user_id = "@alice:example.org"
homeserver = "https://matrix.org"

[[account]]
name = "bob"
user_id = "@bob:example.com"
homeserver = "https://example.com"
```

The active account can be selected via:

```bash
my --account alice crawl
```

---

## 9. Privacy & Trust Model

Core guarantees:

- All data stays local by default
- No analytics, no tracking
- No uploads unless explicitly requested

Agents and contributors **must not** introduce:

- Silent uploads
- Background crawling
- Third-party analytics

---

## 10. For Code-Generation Agents

### Language & stack constraints

This project is implemented in **Rust**.

Mandatory choices:

- Matrix SDK: `matrix-rust-sdk`
- Storage: SQLite (via `rusqlite` or equivalent)
- Async runtime: `tokio`

### Architectural constraints

Agents **must respect**:

- Account isolation (no cross-account reads)
- SQLite as the single source of truth
- Incremental crawling via sync tokens

Agents **must not**:

- Store secrets in the database
- Use global mutable state
- Block the async runtime

When unsure, prefer:

- Explicit schemas
- Small, composable modules
- Clear error propagation

---

## 11. Non-Goals

This project intentionally does **not**:

- Rank users globally
- Compare users
- Provide real-time analytics
- Act as a Matrix client

---

## 12. Attribution

Matrix is an open standard. This project is not affiliated with matrix.org.

