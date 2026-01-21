# CLI Documentation

This document describes the command-line interface for `my` (matrix-year).

## Commands

### `login` / `logout`

Authenticate a Matrix account and securely store credentials.

**Usage:**
```bash
my login [--user-id <@alice:example.org>]
my logout [--user-id <@alice:example.org>]
```

**Login Behavior:**
- Displays existing logged-in accounts for reference (if any).
- Prompts for homeserver, username and password to add a new account.
- Stores credentials locally in `.my/accounts/<account>/meta/credentials.json` with restricted permissions (owner read/write only on Unix).
- Persists session metadata to `.my/accounts/<account>/meta/session.json` and restores sessions automatically on subsequent runs.
- If cross-signing is enabled and the new device is unverified, offers SAS emoji verification or guidance for recovery-key verification.
- Supports multi-account: pass `--user-id` to target a specific account, otherwise an interactive prompt appears after showing existing accounts.

**Logout Behavior:**
- `--user-id` is optional. If omitted, prompts to select from existing accounts.
- Asks for user confirmation displaying the user ID(s) before proceeding.
- Removes stored credentials and deletes local account data from `.my/accounts/<account>/`.

**Examples:**
```bash
my login --user-id @alice:example.org
my logout @alice:example.org
```

### Window Command (Shorthand)

Crawl and render a time window for a single account in one command. This is a convenient shorthand for running `my crawl <window>` followed by `my render --stats <stats_file>`.

**Usage:**
```bash
my <window> [--user-id <@alice:example.org>] [--formats <list>] [--output <dir>]
```

**Arguments:**
- `<window>` — Temporal scope (e.g., `2025`, `2025-03`, `2025-W12`, `2025-03-15`, `life`).

**Options:**
- `--user-id <@alice:example.org>` — (Optional) Target a specific account. If omitted, prompts for selection.
- `--formats <list>` — Comma-separated list of formats (e.g., `md`, `md,html`). Defaults to all available formats.
- `--output <dir>` — Output directory for generated reports. Defaults to current directory.

**Behavior:**
1. **Selects a single account** via interactive prompt (if multiple exist) or `--user-id` flag.
2. **Crawls** the specified window for that account.
3. **Generates stats** file at `{account_dir}/stats-{window}.json`.
4. **Renders** in specified formats to the output directory.

**Examples:**

Crawl and render year 2025:
```bash
my 2025
```

Crawl and render for specific account:
```bash
my 2025 --user-id @alice:example.org
```

Crawl, render, and specify output directory:
```bash
my 2025 --output reports
my 2025 --user-id @alice:example.org --output reports
```

Crawl and render specific formats:
```bash
my 2025 --formats md
my 2025 --formats md --output reports
```

Other windows:
```bash
my 2025-03        # Month
my 2025-W12       # Week  
my 2025-03-15     # Day
my life           # Entire history
```

**Note:** Unlike `my crawl`, the window command only processes **one account** per invocation to ensure a clear crawl→render workflow.

### `status`

Show the status of all logged-in Matrix accounts, including account IDs, homeserver, and session health. Useful for quickly checking which accounts are active and whether credentials are valid.

**Usage:**
```bash
my status [--list] [--user-id <@alice:example.org>]
```

**Options:**
- `--list` — Show detailed room listing with crawl status for each room.
- `--user-id <@alice:example.org>` — (Optional) Target a specific account. If omitted, prompts to select from existing accounts.

**Behavior:**

Without `--list`:
- Lists all accounts found in the data directory.
- For each account, displays:
  - Matrix user ID
  - Homeserver
  - Whether credentials are present and valid
  - Session health (restorable, needs login, etc.)
- Exits with nonzero status if no accounts are found or if any account is in an error state.

With `--list`:
- Shows all rooms with their crawl metadata.
- Each room displays:
  - Status symbol (`○` virgin, `✓` success, `⠧` in progress, `✗` error)
  - Room name (truncated to 40 display columns)
  - Event counts (total and user-sent)
  - Oldest event timestamp
  - `💯` indicator for fully crawled rooms (reached room creation)
- Rooms are sorted by status priority: virgin → success (fully crawled first) → in-progress → error
- Proper Unicode-aware alignment for room names with emoji or multi-byte characters

**Examples:**
```bash
my status
my status --list
my status --list --user-id @alice:example.org
```

### `crawl`

Download Matrix messages from your joined rooms into the local SDK database. The crawl command uses **sliding sync** for efficient room discovery and paginated timeline access to incrementally fetch historical messages.

**Usage:**
```bash
my crawl <window> [--user-id <@alice:example.org>]
```

**Arguments:**
- `<window>` — (Mandatory) Temporal scope for crawling. Accepts:
  - `2025` — Calendar year (e.g., all of 2025)
  - `2025-03` — Month (e.g., March 2025)
  - `2025-W12` — ISO week (e.g., week 12 of 2025)
  - `2025-03-15` — Specific day
  - `life` — All messages from epoch onward (entire message history)
- `--user-id <@alice:example.org>` — (Optional) Crawl a specific logged-in account. If omitted, prompts to select from existing accounts.

**Behavior:**
- **Stage 1:** Discovers rooms via sliding sync (growing mode, batch size 50, 1 event per room to capture latest).
- **Stage 2:** Paginates backward through historical events for rooms that need data within the window (batches of 100, parallel with 8 concurrent rooms).
- **Stage 3:** Builds account-level statistics from crawled events and saves to `.my/accounts/<account>/stats-<window>.json`.
- Shows live progress with animated spinners per room and sticky overall counter.
- Stores all events in the SDK's encrypted SQLite database automatically.
- Generates comprehensive statistics (temporal activity, room rankings, reactions, etc.) saved as JSON.

**Sync Lifecycle:**
- A single sliding sync session runs during crawl execution to discover rooms and capture latest events; behavior is the same for current and historical windows.

**Examples:**

Crawl the current year:
```bash
my crawl 2025
```

Crawl a specific month:
```bash
my crawl 2025-03
```

Crawl a specific week:
```bash
my crawl 2025-W12
```

Crawl a specific day:
```bash
my crawl 2025-03-15
```

Crawl entire message history:
```bash
my crawl life
```

Crawl a specific account (if multiple are logged in):
```bash
my crawl 2025 --user-id @alice:example.org
```


### `reset`

Clear all crawl metadata and SDK data while preserving account credentials. This is useful for troubleshooting, testing fresh crawls, or resetting after SDK database corruption. **Note:** This does not log you out—credentials remain intact.

**Usage:**
```bash
my reset [--user-id <@alice:example.org>]
```

**Arguments:**
- `--user-id <@alice:example.org>` — (Optional) Reset a specific logged-in account. If omitted, prompts to select from existing accounts.


**Examples:**

Reset all accounts:
```bash
my reset
```

Reset a specific account:
```bash
my reset --user-id @alice:example.org
```

### `render`

Generate windowed reports (year, month, week, day, life) in one or more formats from a stats file.

**Usage:**
```bash
my render --stats <path> [--formats <list>] [--output <dir>]
```

**Options:**
- `--stats <path>` — (Required) Path to JSON stats file. The stats file contains all necessary metadata (scope, window, account info).
- `--formats <list>` — Comma-separated list of formats (e.g., `md`, `md,html`). Defaults to all available formats.
- `--output <dir>` — Output directory for generated reports. Defaults to current directory.

**Behavior:**
- Loads stats from the provided file path.
- Generates reports in requested formats (currently only `md` is implemented).
- Filenames are auto-generated based on scope from the stats file:
  - Year: `my-year-2025.md`
  - Month: `my-month-2025-03.md`
  - Week: `my-week-2025-W12.md`
  - Day: `my-day-2025-03-15.md`
  - Life: `my-life.md`

**Examples:**

Render from stats file to current directory:
```bash
my render --stats examples/example-stats.json
```

Render to specific output directory:
```bash
my render --stats examples/example-stats.json --output reports
```

Render specific formats:
```bash
my render --stats examples/example-stats.json --formats md
```

Render different windows:
```bash
my render --stats examples/example-stats-2025-03.json
my render --stats examples/example-stats-2025-W12.json
my render --stats examples/example-stats-2025-03-15.json
my render --stats examples/example-stats-life.json
```

**Sample outputs:**
- [examples/my-year-2025.md](examples/my-year-2025.md)
- [examples/my-month-2025-03.md](examples/my-month-2025-03.md)
- [examples/my-week-2025-W12.md](examples/my-week-2025-W12.md)
- [examples/my-day-2025-03-15.md](examples/my-day-2025-03-15.md)
- [examples/my-life.md](examples/my-life.md)

---

## Development

### Running with Cargo

During development, you can run commands directly using `cargo run`:

```bash
cargo run -- render --stats examples/example-stats.json
```

The `--` separator tells Cargo to pass all following arguments to the `my` binary.

### Integration Tests

The project includes integration tests that validate the login and cross-signing flow. To run them:

**Setup credentials:**

1. Copy the template: `cp .env.template .env`
2. Edit `.env` with your test account credentials
3. Load variables: `source .env`

**Run integration tests:**

```bash
(set -a && source .env && set +a && cargo test --test integration_login -- --ignored --nocapture)
```

**Note:** Never commit the `.env` file as it contains sensitive credentials. Only `.env.template` should be committed.
