# [my] — My year, on Matrix

`matrix-year` (CLI: `my`) generates a **year-in-review** for your Matrix account: activity stats, highlights, and maybe fun insights.

## ✨ What do you get?

From your Matrix account, `my` can generate:

- 📄 **Markdown report**
- 🌐 **Static HTML recap**
- 🎞️ **GIF / visual recap**
- 📊 **JSON stats**

Examples of stats:
- Messages sent, active days, peak months
- Activity by hour and weekday
- Top private rooms & DMs (public rooms excluded)
- Reactions your messages received
- Rooms you created during the year
- Fun stats and habits

## 🚀 CLI usage

### Crawl your data (incremental & resumable)

```bash
my crawl --user @alice:example.org
```

### Build yearly stats

```bash
my stats --year 2025
```

### Render your recap

```bash
my render md   --year 2025
my render web  --year 2025
my render gif  --year 2025
my render json --year 2025
```

## 📐 Stable stats model

- Spec: **`STATS_SPEC.md`**
- JSON Schema (Draft-07)
- Renderer-agnostic


## 🤝 Contributing / Agents

See **`AGENTS.md`**.
