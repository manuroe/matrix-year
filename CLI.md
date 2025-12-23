# CLI Documentation

This document describes the command-line interface for `my` (matrix-year).

## Commands

### `--render`

Generate year-in-review reports in one or more formats.

**Usage:**
```bash
my --render [formats] --json-stats <path> [--output <dir>]
```

**Arguments:**
- `--render [formats]` — Comma-separated list of formats to render (e.g., `md`, `md,html`). If omitted after flag or left empty, renders all available formats.
- `--json-stats <path>` — (Optional, required for now) Path to JSON statistics file. In the future, stats will be read from the database.
- `--output <dir>` — (Optional) Output directory for generated reports. Defaults to current directory. Filenames are generated automatically (e.g., `my-year-2025.md`).

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

**Sample output:** See [examples/my-year-2025.md](examples/my-year-2025.md)

---

## Development

### Running with Cargo

During development, you can run commands directly using `cargo run`:

```bash
cargo run -- --render md --json-stats examples/example-stats.json
```

The `--` separator tells Cargo to pass all following arguments to the `my` binary.
