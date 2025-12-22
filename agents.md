# AGENTS.md

[my] — My year, on Matrix.

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

### Matrix SDK Storage

Raw Matrix data is **managed by the Matrix SDK** (`matrix-rust-sdk`).

The SDK:

- Stores events in its own encrypted database
- Handles end-to-end encryption automatically
- Manages sync state and event persistence

Secrets (access tokens, database encryption keys) **must be stored securely** using platform-appropriate facilities:

- **macOS:** Keychain
- **Linux:** Secret Service API (libsecret) or similar
- **Windows:** Credential Manager

Agents **must not** store secrets in plaintext or in the stats database.

---

### Stats Cache (SQLite)

Each account has a **separate SQLite database for derived statistics**:

```text
accounts/<account>/db.sqlite
```

Purpose:

- Cache computed yearly statistics
- Avoid re-parsing all Matrix events on subsequent requests
- Store metadata (last computation time, data version)

This database **does not** store:

- Raw Matrix events (handled by SDK)
- Message content
- Encryption keys or tokens

Render outputs remain **outside** the database.

---

## 4. Event Access

### Storage model

Raw Matrix events are **managed entirely by the Matrix SDK**.

The SDK provides:

- Encrypted storage of events and room state
- Automatic sync token management
- Deduplication of events
- Query interfaces for accessing historical data

The project **does not** directly manage Matrix event storage.

### Stats cache schema (conceptual)

The project's SQLite database stores **derived statistics only**:

```sql
stats_cache(
  year INTEGER,
  account_id TEXT,
  computed_at INTEGER,
  stats_json TEXT,
  PRIMARY KEY (year, account_id)
)

meta(
  key TEXT PRIMARY KEY,
  value TEXT
)
```

### Guarantees

- Stats are deterministic for a given event set
- Stats can be recomputed from SDK data at any time
- No message content is stored in the stats cache

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

### Command-line driven

All configuration is provided via **command-line arguments**. No configuration files are used.

### Authentication

**Login to an account:**

```bash
my login @alice:example.org
```

**Logout from an account:**

```bash
my logout @alice:example.org
```

This command:
- Removes stored credentials
- Removes local data

### Core Commands

**Crawl Matrix data:**

```bash
my crawl                                    # Crawl all logged-in accounts
my crawl --user-id @alice:example.org       # Crawl specific account
my crawl --until 2025-01-01                 # Crawl events with timestamps up to and including this date
```

**Generate statistics:**

```bash
my stats 2025                               # Generate stats for all accounts
my stats 2025 --user-id @alice:example.org  # Generate stats for specific account
```

### Multi-account support

Accounts are identified by their Matrix user ID and stored in separate directories.

If no account is specified, commands run for **all logged-in accounts** found in the data directory.

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

## 11. Git & GitHub Workflow

### Git User Configuration

All git operations **must** use the git user configured on the machine.

Verify user configuration with:

```bash
git config user.name
git config user.email
```

Use these credentials for all commits and git operations.

---

### Pull Request Workflow

When asked to create a pull request, agents **must** follow this sequence. If the changes are logically independent, create several commits to simplify the review process:

#### 1. Document Session Prompts

Create or update `PROMPTS.md` with a new section containing:

- **Section title:** The PR title
- **Content:** All user prompts from the current session, in chronological order
- **Format:** Clear separation between prompts, with timestamps if available

#### 2. Update Agent Documentation

Amend `AGENTS.md` with:

- Any architectural insights learned during implementation
- New constraints or patterns discovered
- Edge cases or clarifications
- Updated examples if applicable

Keep changes focused and avoid redundancy.

#### 3. Run Quality Checks

Execute all project linters and tests:
Ensure all checks pass before proceeding.

#### 4. Request Validation

**Before creating the PR**, present the `PROMPTS.md` additions to the user and ask:

> I've documented the session prompts in PROMPTS.md. Please review the following additions:
>
> [show PROMPTS.md section]
>
> Should I proceed with creating the pull request?

Wait for explicit confirmation before creating the PR.

#### 5. Create Pull Request

Only after validation:

- Commit all changes with descriptive message
- Push to feature branch
- Create PR with appropriate title and description
- Reference any related issues

---

## 12. Non-Goals

This project intentionally does **not**:

- Rank users globally
- Compare users
- Provide real-time analytics
- Act as a Matrix client

---

## 13. Attribution

Matrix is an open standard. This project is not affiliated with matrix.org.

