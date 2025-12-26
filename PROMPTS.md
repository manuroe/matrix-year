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

## PR: Windowed Reports (Year, Month, Week, Day, Life)

### Prompt 1: Extend tool to support multiple time windows
> Extend the tool to build reports per year but also per month, per week, per day and per life (since the account creation). The reports should adapt given the window. Create json examples for those new stats. Their generated report should be part of the project.

### Prompt 2: Keep README focused on year-in-review
> Keep the readme as is, mainly. The main point is still a year-in-review. We can just add somewhere a "oh, by the way, `[my]` supports building monthly and other reports"

### Prompt 3: Remove redundant scope phrase from life report
> This line means nothing for life report

### Prompt 4: Add peak month year for context
> Give the year of the peak month

### Prompt 5: Include message count in rooms section
> Give the number of messages: "You sent X messages in **6** rooms". Apply it for all reports

## PR: Peak Activity Section

### Prompt 1: Add peak section with peak year/month/week/day/hour
> Add a peek section sent message in the reports with (when it applies):
> - peak year
> - peak month
> - peak week
> - peak day


> - peak hour
> Give the number of messages

### Prompt 2: Rename section and move under Activity
> Make "🚀 Peak activity" the first sub-section of "📈 Activity". Call it peaks.
> We need the date and time for the peak hour

### Prompt 3: Add week and day peaks to all examples
> I want week and day peaks in the examples.
> All examples must have them when they make sense.
> Make sure the data is coherent and respects the message count hierarchy (peak hour ≤ peak day ≤ peak week ≤ peak month ≤ peak year).

### Prompt 4: Remove backward compatibility
> don't take care of backward compatibility

### Prompt 5: Make peak hour date mandatory
> it is mandatory to make it more useful

### Prompt 6: Generate all example reports
> generate reports for all the examples

### Prompt 7: Remove rooms breakdown table
> This text is not useful remove it

### Prompt 8: Ensure consistent fun facts across reports
> Fun facts should be the same for all the reports

### Prompt 9: Remove deprecated year field
> It is deprecated, remove it. In general we do not need to maintain retro compatibility because we regenerate the report on each change. Update the docs to indicate we do not need compat for stats json and renderers.

## PR: Markdown Renderer Formatting Tweaks

### Prompt 1: Refine markdown report titles and sections
> Few tweaks for the MD report:
> - change the title report to "# 🎉 Your Matrix Year 2025 — Alice"
> - use https:// link instead of mxc://
> - "🏠 **Active rooms:**" is not clear. Remove it. We have the info in the next section.
> - in "### 🏗️ Rooms You Created", remove "- **Total:** 8". Add a sentence "You created **2 rooms** this year.", "this year" or the given window

### Prompt 2: Start implementation
> Start implementation

### Prompt 3: Create pull request
> PR it

## PR: Document Rust quality bar

- Check the quality of the code. It must be rust idiomatic and must look like built by a senior rust dev would do. `unsafe` must not be used for example. Update agents.md to keep the standard high.
- PR the current change following the agents.md guide using a separate branch.

---

## Add login, logout and status commands

- Implement `my login` using the latest `matrix-rust-sdk` with interactive prompts and secure storage.
- Store credentials (user id, device id, access token, refresh token, DB encryption passphrase) in OS keychain; JSON fallback with warnings.
- Enable multi-account UX via `--user-id` and interactive selection when omitted.
- Ensure session restore using `restore_session` with `SessionMeta` and `SessionTokens`.
- Upgrade to `matrix-sdk = 0.16.0`; refactor login flow to new APIs and validate builds.
- ... Too many. It lost the context
