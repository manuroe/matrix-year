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

1. `MY_DATA_DIR` environment variable
2. Current working directory (`./.my/`)

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

**Credentials storage** (access tokens, refresh tokens, database encryption keys) is managed via the `AccountSecretsStore` abstraction in `src/secrets.rs`. 

Current implementation:
- Stores credentials in local JSON files at `accounts/{account}/meta/credentials.json`
- File permissions restricted to owner-only (0600 on Unix)
- Storage mechanism is completely encapsulated in `secrets.rs`

The abstraction allows switching storage backends (keychain, encrypted files, etc.) without changing other modules.

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
  scope_type TEXT,
  scope_key TEXT,
  account_id TEXT,
  computed_at INTEGER,
  stats_json TEXT,
  PRIMARY KEY (scope_type, scope_key, account_id)
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

Convert raw events into **scope-aware derived statistics** (year, month, week, day, life).

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
- Rooms ranking includes **DM**, **private**, and **public** rooms, counting only messages **sent by the account**; top room entries must carry a permalink.
- Optional distribution `messages_by_room_type` (dm/private/public) may be emitted for the Rooms section; renderers may omit it in non-`full` modes.
- **Peak activity** (strongest periods per granularity) is included in summary via `peaks` object with optional fields for year, month, week, day, and hour. Peak hour must include the calendar date (local time) to provide temporal context.

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

Login process:
- Authenticates with homeserver via username/password
- Stores session tokens and metadata locally
- Initializes end-to-end encryption automatically
- If cross-signing is enabled on the account, prompts to cross-sign the new session via:
  - **SAS emoji verification**: Compare emojis with another verified device
  - **Recovery key/passphrase**: Unlock secret storage to import cross-signing keys
- Recovery credentials are transient (used only during login, never stored)

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

**Render reports:**

```bash
my --render md --json-stats <path>                      # Render Markdown to current directory
my --render md --json-stats <path> --output <dir>       # Render Markdown to specific directory
my --render md,html --json-stats <path> --output <dir>  # Render multiple formats
```

Output filenames are generated automatically (e.g., `my-year-2025.md`). The `--json-stats` flag is currently required for development; future versions will read from the database.

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
- Session persistence via SDK sessions (access + refresh tokens)
- Credential storage abstraction via `AccountSecretsStore`
- Cross-signing verification flow during login (SAS emoji or recovery key)

Agents **must not**:

- Store secrets in the database
- Use global mutable state
- Block the async runtime
- Access credential storage directly; use `AccountSecretsStore` API only
- Store recovery keys or passphrases (they are transient, used only during session cross-signing)

When unsure, prefer:

- Explicit schemas
- Small, composable modules
- Clear error propagation

### Rust quality bar

- No `unsafe`; avoid panics except in tests and build-time invariants.
- Treat all I/O with context (`anyhow::Context`) so failures are diagnosable.
- Prefer borrowing over cloning; use `&Path`/`&PathBuf` instead of `String` for filesystem inputs.
- Keep CLI help and runtime behavior in sync; rely on `clap`-generated help where possible to avoid drift.
- Run `cargo fmt` and `cargo clippy --all-targets --all-features -D warnings` before merging.
- Handle authentication errors with context; ensure session restore paths use `restore_session` with `SessionMeta` and `SessionTokens`.
- Add focused tests when touching stats schema, rendering logic, or CLI parsing; keep example outputs up to date when behavior changes.

### Integration Tests

The project includes integration tests that require live Matrix account credentials to verify the full login, encryption, and cross-signing flows.

**Test isolation:**
- Tests use the `MY_DATA_DIR` environment variable to store data in a temporary directory
- This prevents pollution of the default `~/.my` directory
- Each test run creates a fresh temporary directory that is automatically cleaned up

**Running the integration test locally:**

1. Set up credentials in `.env`:
   ```bash
   cp .env.template .env
   # Edit .env with your test account credentials
   ```

2. Run the integration test:
   ```bash
   (set -a && source .env && set +a && cargo test --test integration_login -- --ignored --nocapture 2>&1)
   ```

   This command:
   - Loads credentials from `.env` without exposing them in shell history
   - Runs the ignored integration test with output
   - Tests the complete flow: login → encryption → recovery key verification → status checks
   - Uses a temporary directory for all data (via `MY_DATA_DIR` set by the test)

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

#### 1. Quality Assurance

Before proceeding, verify **consistency across three dimensions**:

- **Code:** Implementation correctly reflects the intended behavior
- **Docs:** CLI.md and agents.md document all user-facing changes
- **Examples:** Run all examples to ensure generated output is current and valid

Verify documentation is up-to-date:
- Check CLI.md accurately reflects current command behavior and options
- Ensure agents.md captures any architectural changes or new constraints
- Confirm all command examples in docs match actual implementation

Rebuild all examples and validate output:
```bash
cargo run -- --render md --json-stats examples/example-stats.json --output examples
```

#### 2. Document Session Prompts

Create or update `PROMPTS.md` with a new section containing:

- **Section title:** The PR title
- **Content:** All user prompts from the current session, in chronological order
- **Format:** Clear separation between prompts, with timestamps if available

#### 3. Update Agent Documentation

Amend `AGENTS.md` with:

- Any architectural insights learned during implementation
- New constraints or patterns discovered
- Edge cases or clarifications
- Updated examples if applicable

Keep changes focused and avoid redundancy.

#### 4. Run Quality Checks

Execute all project linters and tests:
Ensure all checks pass before proceeding.

#### 5. Apply Code Formatting

Format all Rust code:
```bash
cargo fmt
```

Commit formatting changes with a separate "chore: apply cargo fmt" commit before proceeding.

#### 6. Request Validation

**Before creating the PR**, present the `PROMPTS.md` additions to the user and ask:

> I've documented the session prompts in PROMPTS.md. Please review the following additions:
>
> [show PROMPTS.md section]
>
> Should I proceed with creating the pull request?

Wait for explicit confirmation before creating the PR.

#### 7. Create Pull Request


Only after validation:

- Commit all changes with descriptive message
- Push to feature branch
- Create PR with appropriate title and description using the GitHub CLI (`gh`)
- Reference any related issues

---

### Addressing PR Review Comments

When asked to address PR review comments, agents **must** follow this workflow:

#### 1. Read Comments

Fetch PR comments via the public URL:

```bash
# Example: https://github.com/manuroe/matrix-year/pull/10
```

The project is public, so comments are accessible without authentication. Use the `fetch_webpage` tool to retrieve specific discussion threads:

```bash
# Example discussion URL
https://github.com/manuroe/matrix-year/pull/10#discussion_r2648705875
```

#### 2. Apply Fixes

- Address **each comment in a separate commit** for clear traceability
- Keep fixes aligned with project coding standards (see Rust quality bar)
- Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test --all-features` after each fix
- Commit fixes with clear messages referencing the discussion URL

Example commit message:
```
fix(login): move entire account directory to prevent SDK database loss

When the server returns a different user ID format than the hint, move the 
entire account directory (including sdk/) instead of just session.json to 
prevent losing the SDK database with encryption keys and sync state.

Addresses: https://github.com/manuroe/matrix-year/pull/12#discussion_r2659498410
```

#### 3. Comment on Review Threads

After pushing fixes, add a reply to each review comment thread:

- Use the GitHub API to reply directly to the review comment
- Include the commit SHA that fixes the issue
- Briefly explain what was changed

Example using GitHub API:
```bash
gh api \
  --method POST \
  -H "Accept: application/vnd.github+json" \
  /repos/manuroe/matrix-year/pulls/12/comments/2659498410/replies \
  -f body="Fixed in 807aae4 - Now moves the entire account directory (including sdk/) instead of just session.json to prevent losing the SDK database with encryption keys and sync state."
```

#### 4. Mark Comments as Resolved

After commenting with the fix:

- Navigate to the PR discussion thread
- Mark each addressed comment as "Resolved"
- The commit reference in the comment provides clear traceability for reviewers

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

