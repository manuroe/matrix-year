# CLI Documentation

This document describes the command-line interface for `my` (matrix-year).

## Commands

### `--render`

Generate windowed reports (year, month, week, day, life) in one or more formats.

**Usage:**
```bash
my --render [formats] --json-stats <path> [--output <dir>]
```

**Arguments:**
- `--render [formats]` — Comma-separated list of formats to render (e.g., `md`, `md,html`). If omitted after flag or left empty, renders all available formats.
- `--json-stats <path>` — (Optional, required for now) Path to JSON statistics file. Stats **must** include `scope` (`year|month|week|day|life`) and `scope.key`.
- `--output <dir>` — (Optional) Output directory for generated reports. Defaults to current directory. Filenames are generated automatically based on the scope (e.g., `my-year-2025.md`, `my-month-2025-03.md`, `my-week-2025-W12.md`, `my-day-2025-03-15.md`, `my-life.md`).

**Examples:**

Render Markdown report:
```bash
my --render md --json-stats examples/example-stats.json --output examples
```

Render multiple formats:
```bash
my --render md,html --json-stats examples/example-stats.json --output reports
```

Render to current directory:
```bash
my --render md --json-stats examples/example-stats.json
```

Render other windows:
```bash
my --render md --json-stats examples/example-stats-2025-03.json --output examples
my --render md --json-stats examples/example-stats-2025-W12.json --output examples
my --render md --json-stats examples/example-stats-2025-03-15.json --output examples
my --render md --json-stats examples/example-stats-life.json --output examples
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
cargo run -- --render md --json-stats examples/example-stats.json
```

The `--` separator tells Cargo to pass all following arguments to the `my` binary.
