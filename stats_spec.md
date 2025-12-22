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

Statistics are computed **per account, per calendar year**.

Rules:

- Year is determined by `origin_server_ts`
- Partial years are allowed
- Missing data must be represented explicitly

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

  "peak_month": {
    "month": "October",
    "messages": 512
  }
}
```

Rules:
- `messages_sent` and `active_rooms` are core fields
- `dm_rooms`, `public_rooms`, `private_rooms` are **extended fields**
- Extended fields:
  - Must be present in stats if computable
  - May be omitted by renderers in non-`full` modes
- Room counts must be consistent with `rooms.total`

---


### 4. Activity

Describes activity over time.

```json
"activity": {
  "by_month": {
    "01": 320,
    "02": 410,
    "03": 380
  },
  "by_weekday": {
    "Mon": 620,
    "Tue": 700,
    "Wed": 690,
    "Thu": 810,
    "Fri": 650,
    "Sat": 400,
    "Sun": 362
  },
  "by_hour": {
    "00": 42,
    "21": 612,
    "22": 580
  }
}
```

Rules:
- Missing buckets must be omitted or zeroed
- Hours are 00–23, local to the user

---

### 5. Rooms

Ranks **non-public rooms** by activity.

This section explicitly **excludes public rooms** to limit crawl scope and cost.

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
- Only **private rooms and DMs** are included
- Public rooms **must not** be crawled for this section
- Sorted descending by `messages`
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

- New fields must be added in a backwards-compatible way
- `schema_version` must be incremented for breaking changes
- Renderers must ignore unknown fields

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

## Appendix A — JSON Schema (Draft-07)

The following JSON Schema defines the canonical, renderer-facing stats object.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "matrix-year statistics",
  "type": "object",
  "required": [
    "schema_version",
    "year",
    "generated_at",
    "account",
    "coverage",
    "summary"
  ],
  "additionalProperties": false,
  "properties": {
    "schema_version": {
      "type": "integer",
      "minimum": 1
    },

    "year": {
      "type": "integer",
      "minimum": 2000
    },

    "generated_at": {
      "type": "string",
      "format": "date"
    },

    "account": {
      "type": "object",
      "required": ["user_id", "rooms_total"],
      "additionalProperties": false,
      "properties": {
        "user_id": { "type": "string" },
        "display_name": { "type": ["string", "null"] },
        "avatar_url": { "type": ["string", "null"] },
        "rooms_total": { "type": "integer", "minimum": 0 }
      }
    },

    "coverage": {
      "type": "object",
      "required": ["from", "to"],
      "additionalProperties": false,
      "properties": {
        "from": { "type": "string", "format": "date" },
        "to": { "type": "string", "format": "date" },
        "days_active": { "type": "integer", "minimum": 0 }
      }
    },

    "summary": {
      "type": "object",
      "required": ["messages_sent", "active_rooms"],
      "additionalProperties": false,
      "properties": {
        "messages_sent": { "type": "integer", "minimum": 0 },
        "active_rooms": { "type": "integer", "minimum": 0 },

        "dm_rooms": { "type": "integer", "minimum": 0 },
        "public_rooms": { "type": "integer", "minimum": 0 },
        "private_rooms": { "type": "integer", "minimum": 0 },

        "peak_month": {
          "type": "object",
          "required": ["month", "messages"],
          "additionalProperties": false,
          "properties": {
            "month": { "type": "string" },
            "messages": { "type": "integer", "minimum": 0 }
          }
        }
      }
    },

    "activity": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "by_month": {
          "type": "object",
          "additionalProperties": { "type": "integer", "minimum": 0 }
        },
        "by_weekday": {
          "type": "object",
          "additionalProperties": { "type": "integer", "minimum": 0 }
        },
        "by_hour": {
          "type": "object",
          "additionalProperties": { "type": "integer", "minimum": 0 }
        }
      }
    },

    "rooms": {
      "type": "object",
      "required": ["total"],
      "additionalProperties": false,
      "description": "Activity in non-public rooms only (DMs and private rooms). Public rooms are explicitly excluded.",
      "properties": {
        "total": { "type": "integer", "minimum": 0 },
        "top": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["messages"],
            "additionalProperties": false,
            "properties": {
              "name": { "type": ["string", "null"] },
              "messages": { "type": "integer", "minimum": 0 },
              "percentage": { "type": "number", "minimum": 0, "maximum": 100 }
            }
          }
        }
      }
    },

    "reactions": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "total": { "type": "integer", "minimum": 0 },
        "top_emojis": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["emoji", "count"],
            "additionalProperties": false,
            "properties": {
              "emoji": { "type": "string" },
              "count": { "type": "integer", "minimum": 0 }
            }
          }
        },
        "top_messages": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["permalink", "reaction_count"],
            "additionalProperties": false,
            "properties": {
              "permalink": { "type": "string", "format": "uri" },
              "reaction_count": { "type": "integer", "minimum": 0 }
            }
          }
        }
      }
    },

    "created_rooms": {
      "type": "object",
      "required": ["total"],
      "additionalProperties": false,
      "properties": {
        "total": { "type": "integer", "minimum": 0 },
        "dm_rooms": { "type": "integer", "minimum": 0 },
        "public_rooms": { "type": "integer", "minimum": 0 },
        "private_rooms": { "type": "integer", "minimum": 0 }
      }
    },

    "fun": {
      "type": "object",
      "additionalProperties": true
    }
  }
}
```

