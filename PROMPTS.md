# Docs: CLI auth workflow and stats schema

- Review the existing documentation for the project and check consistency/completeness; consolidate docs for later implementation.
- Update agents.md to use the git user defined on the machine for git commands.
- Add a section to AGENTS.md about git and GitHub (use configured git user; PR workflow: document prompts, amend AGENTS.md, run linters/tests, ask for validation before PR).
- Rewrite storage section to clarify Matrix data is managed by the SDK with E2E encryption; secrets stored securely; SQLite cache only for stats.
- Move JSON schema to a dedicated file (stats_schema.json) and reference from stats_spec.md.
- Remove TOML config; require all data via CLI; default to all accounts when unspecified.
- Add dedicated login/logout commands; keep crawl/stats commands; add --until date option for crawl.
- Emphasize creating several commits when changes are independent; request to create a PR.

## PR: CLI render flag, help, docs and examples refresh

- Add matrix.to links for user IDs in Account section.
- Keep title plain (no permalink) and render encrypted ratio in fun facts; rename label to "Encrypted messages".
- Add Summary year note; reorder sections: Account, Summary, Rooms, Created rooms, Reactions, Activity, Fun.
- Merge header and account details; format created rooms with emojis aligned like Summary.
- Introduce --render flag with optional formats, directory --output, optional --json-stats; add two-level --help (global and render topic).
- Simplify CLI docs, add CLI.md; update agents.md for new render usage and PR QA steps; regenerate examples.

## PR: JSON Schema validation for example stats

- can we have a linter for example-stats.json using stats_schema.json?
- Start implementation
- pr it

## PR: Include public rooms and room-type distribution

- I actually want to consider public rooms. The crawling cost will be the same as we will only fetch messages sent by our user. Update the docs and the data about it. We should have now an info in the stats report about the repartition of sent messages between DMs, private rooms and public rooms.
- Start implementation
- This stats should be under the rooms section. Also, call the Rooms section just Rooms and add intro text saying: "You sent messages in X rooms"
- align the RoomEntry.permalink requirement with the Rust struct (optional), or leave as-is for now? The permalink is not optional
- PR it
