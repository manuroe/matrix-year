# Statistics Specification

This document specifies the **canonical statistics model** produced by `matrix-year` (`my`).

It defines:

- What statistics the tool computes
- Their meaning and constraints
- The **JSON schema** consumed by all renderers (Markdown, html, GIF, …)

This is a **normative specification** for AI agents and contributors.

---

## 1. Scope and Philosophy

The statistics layer sits **between crawling and rendering**.

```text
Crawl (Matrix events)
        ↓
Stats Engine (this spec)
        ↓
Renderers (MD, html, GIF, …)
```

Core principles:

- Stats are **derived**, never raw
- Stats are **renderer-agnostic**
- Stats are **stable across versions** when possible
- Stats are **privacy-preserving by design**

---

## 2. Privacy Constraints (Hard Rules)

The stats engine **must not** produce:

- Message bodies
- Media URLs
- Precise timestamps tied to single messages

Allowed data:

- Aggregated counts
- Rankings
- Buckets (per day, month, hour)
- Rounded or coarse-grained timestamps

Renderers **must not** attempt to reconstruct raw activity.

---

## 3. Temporal Scope

Statistics are computed **per account** for a specific **time window**.

- Supported scopes: `year`, `month`, `week`, `day`, `life` (since account creation)
- Scope is expressed via `scope.type` and `scope.key`
  - `year`: `scope.key = "2025"`
  - `month`: `scope.key = "2025-03"` (YYYY-MM)
  - `week`: `scope.key = "2025-W12"` (ISO week)
  - `day`: `scope.key = "2025-03-15"` (YYYY-MM-DD)
  - `life`: `scope.key = "life"`
- `scope.label` may be provided for rendering; otherwise renderers derive a friendly label from `type` and `key`.

Rules:

- Coverage (`coverage.from` / `coverage.to`) must match the window when applicable
- Partial coverage is allowed but must be explicit via coverage dates
- Renderers must not assume yearly context; they must use `scope`

---

## 4. Generated Report Structure

This section defines the **canonical structure and semantic ordering** of the generated statistics report.

Renderers **must respect this logical order**, even if the visual layout differs.

The numbering below is **semantic** and part of the contract.

---

### 1. Account

Describes the Matrix account the statistics were generated for.

Provides **identity and context**, not activity.

```json
"account": {
  "user_id": "@alice:example.org",
  "display_name": "Alice",
  "avatar_url": "mxc://example.org/abcdef",
  "rooms_total": 27
}
```

Rules:
- `user_id` is required and authoritative
- `display_name` is optional and may be stale
- `avatar_url` must be an MXC URI or null
- `rooms_total` is the total number of joined rooms, including inactive ones

---

### 2. Coverage

Describes how complete the underlying data is.

```json
"coverage": {
  "from": "2025-01-02",
  "to": "2025-12-19",
  "days_active": 220
}
```

Rules:
- Dates are ISO-8601 (YYYY-MM-DD)
- Coverage must reflect crawled data, not assumptions

---

### 3. Summary

High-level overview suitable for quick rendering.

This section contains **core stats** (always rendered) and **extended stats** (rendered only in `full` mode).

```json
"summary": {
  "messages_sent": 4832,
  "active_rooms": 12,

  "dm_rooms": 5,
  "public_rooms": 4,
  "private_rooms": 3,

  "peaks": {
    "year": { "year": "2022", "messages": 5200 },
    "month": { "month": "October", "messages": 512 },
    "week": { "week": "2025-W12", "messages": 210 },
    "day": { "day": "2025-03-15", "messages": 42 },
    "hour": { "hour": "21", "messages": 36, "date": "2025-10-21" }
  }
}
```

Rules:
- `messages_sent` and `active_rooms` are core fields
- `dm_rooms`, `public_rooms`, `private_rooms` are **extended fields**
- Extended fields:
  - Must be present in stats if computable
  - May be omitted by renderers in non-`full` modes
- `peaks` groups the strongest activity per period; include only the granularities that make sense for the scope (e.g., `year`/`month`/`hour` for `life`, `week`/`day`/`hour` for `month` scope)
- `peaks.hour.date` is mandatory and must provide the calendar date of that hour (local time)
- Room counts must be consistent with `rooms.total`


---


### 4. Activity

Describes activity over time.

```json
"activity": {
  "by_year": { "2023": 1200, "2024": 3600 },
  "by_month": { "01": 320, "02": 410, "03": 380 },
  "by_week": { "2025-W12": 210, "2025-W13": 180 },
  "by_weekday": { "Mon": 620, "Tue": 700, "Wed": 690, "Thu": 810, "Fri": 650, "Sat": 400, "Sun": 362 },
  "by_day": { "01": 42, "02": 68 },
  "by_hour": { "00": 42, "21": 612, "22": 580 }
}
```

Rules:
- Missing buckets must be omitted or zeroed
- Hours are 00–23, local to the user
- Renderers should pick the buckets that best fit the scope:
  - `year` / `life`: favor `by_month`, `by_year`, `by_weekday`, `by_hour`
  - `month`: favor `by_day`, `by_weekday`, `by_hour`
  - `week`: favor `by_weekday`, `by_hour`
  - `day`: favor `by_hour`

---

### 5. Rooms

Ranks rooms by activity.

Public rooms are **included**. Crawl and computation only consider **messages sent by the account**, so cost and privacy remain bounded.

Additional distribution (recommended):

```json
"messages_by_room_type": {
  "dm": 1620,
  "private": 2310,
  "public": 902
}
```

Rules:
- Counts represent **messages sent by the account** during the year
- Keys: `dm`, `private`, `public` (integers ≥ 0)
- Sum must equal `summary.messages_sent`
- Renderers may omit this in non-`full` modes

```json
"rooms": {
  "total": 12,
  "top": [
    {
      "name": "Friends",
      "messages": 900,
      "percentage": 18.7
    }
  ]
}
```

Rules:
- Includes **DM**, **private**, and **public** rooms
- Sorted descending by `messages` (sent by the account)
- Limited to top N (default: 5)
- Room names may be omitted for privacy

---


### 6. Reactions

Captures emoji-based interactions **and engagement with your messages**.

This section contains both **aggregate reaction stats** and **top reacted messages**.

```json
"reactions": {
  "total": 1120,

  "top_emojis": [
    { "emoji": "😂", "count": 180 },
    { "emoji": "👍", "count": 140 }
  ],

  "top_messages": [
    {
      "permalink": "https://matrix.to/#/!roomid:example.org/$eventid",
      "reaction_count": 42
    }
  ]
}
```

Rules:
- `top_emojis`:
  - Sorted descending by `count`
  - Limited to top N (default: 5)
- `top_messages`:
  - Sorted descending by `reaction_count`
  - Limited to top N (default: 5)
  - Must reference **messages sent by the account**
  - `permalink` must be a valid matrix.to URL
  - No message content, event IDs, or timestamps are exposed
- Renderers may omit `top_messages` outside of `full` mode

---


### 7. Created Rooms

Describes rooms **created by the account during the year**.

This section contains **counts only**. No room identifiers or names are ever included.

Stats may be rendered only in `full` mode.

```json
"created_rooms": {
  "total": 2,
  "dm_rooms": 0,
  "public_rooms": 1,
  "private_rooms": 1
}
```

Rules:
- Only rooms created by the account during the year are counted
- No room IDs, names, or permalinks are allowed
- `dm_rooms`, `public_rooms`, and `private_rooms` are optional but recommended
- Counts must sum consistently with `total` when present

---


### 8. Fun

Optional, playful statistics.


### 8. Fun

Optional, playful statistics.

```json
"fun": {
  "longest_message_chars": 1024,
  "favorite_weekday": "Thu",
  "peak_hour": "21",
  "longest_streak_days": 15
}
```

Rules:
- All fields are optional
- Precision must be coarse and human-friendly

---

## 5. Extensibility

- `schema_version` must be incremented for any schema changes
- **No backward compatibility guarantee:** stats JSON and renderers are regenerated on each change
- New fields may be added or modified without compatibility concerns
- Renderers must ignore unknown fields for forward compatibility


---


## 6. Determinism & Stability

Given the same event set:

- Stats output must be deterministic
- Ordering of arrays must be stable
- Floating-point values must be rounded consistently

---

## 7. Renderer Contract

Renderers:

- Consume only the stats object defined here
- Must not require raw events
- Must gracefully handle missing sections

---

## 8. Agent Guidelines

When generating or modifying stats code:

- Follow this spec exactly
- Prefer adding fields over changing semantics
- Treat this file as the source of truth


---

## Appendix A — JSON Schema

The machine-readable JSON Schema (Draft-07) is available in [stats_schema.json](stats_schema.json).

This schema can be used for:
- Runtime validation in code
- Code generation (schema → types)
- IDE autocomplete and validation
- Testing fixture validation

