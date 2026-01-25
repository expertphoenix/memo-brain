---
name: memo-brain
description: Manages contextual memories using a vector database CLI tool. Records valuable experiences, solutions, and user habits during conversations. Searches semantic memories to provide context-aware assistance. Use when needing to remember or recall information across conversations, or when the user asks to remember something or search memories.
---

# Memo Brain Management

Record and retrieve valuable knowledge using vector database semantic search.

## Commands

| Command | Purpose | Example |
|---------|---------|---------|
| `memo embed <input>` | Record memory | `memo embed "solution..." --tags rust,cli` |
| `memo search <query>` | Search memories | `memo search "rust best practices" -n 5` |
| `memo list` | List all memories | `memo list` |

**Note:** These commands require network access for embedding API. When using Shell tool, set `required_permissions: ["network"]` or `["all"]`.

## Common Parameters

| Parameter | Purpose | Default |
|-----------|---------|---------|
| `-t, --tags` | Add tags (comma-separated) | - |
| `-n, --limit` | Result count | 5 |
| `--threshold` | Similarity threshold (0-1) | 0.7 |
| `--after` | Filter by time (YYYY-MM-DD) | - |
| `--before` | Filter by time (YYYY-MM-DD) | - |

---

## When to Record

### ✅ Record When

| Situation | Example |
|-----------|---------|
| Solved complex problem | Debugging async lifetime issues |
| Found clever solution | Non-obvious workaround or pattern |
| User says explicitly | "记住这个", "记录下来" |
| Tool/library config | Setting up build tools, API config |
| User preferences | Coding style, library choices |

### ❌ Don't Record

- Simple syntax queries
- Common knowledge
- Temporary workarounds
- Duplicate content (search first)

## Content Format

**Template:**
```
[Topic] - [Short Title]

背景：[1-2 sentences describing context]
方案：[Specific solution or knowledge]
关键点：[Key points or gotchas]
```

**Examples:**

Text:
```bash
memo embed "Rust async trait - Use async-trait crate

背景：Direct async fn in trait causes compile error
方案：Use #[async_trait] macro on trait and impl
关键点：Both trait definition and impl need the macro" --tags rust,async
```

File:
```bash
memo embed notes.md --tags important
```

Directory:
```bash
memo embed ./docs --tags documentation
```

---

## When to Search

### Trigger Scenarios

| Trigger | Example Query |
|---------|---------------|
| Similar past problem | "之前是怎么解决的" |
| Recall specific solution | "那个配置怎么写的" |
| User asks explicitly | "查一下记忆", "还记得吗" |
| Start new task | Check for related experience |
| Recent work | "最近有没有..." (use `--after`) |

### Search Strategy

```bash
# Standard search
memo search "rust async" -n 5

# More results
memo search "database optimization" -n 10

# Lower threshold for broader matches
memo search "error handling" --threshold 0.6

# Time-based filtering
memo search "recent fixes" --after 2026-01-20

# Combined
memo search "vue patterns" -n 10 --after 2026-01-20 --threshold 0.6
```

### Parameter Guidelines

| Parameter | Typical | When to Adjust |
|-----------|---------|----------------|
| `-n` | 5 | Use 10-20 for complex topics |
| `--threshold` | 0.7 | Lower to 0.6-0.5 if no results |
| `--after` | - | Find recent or time-specific memories |

### Using Results

1. Parse returned memories by similarity score
2. Extract relevant parts for current context
3. Cite sources: similarity score, update time, tags

---

## Best Practices

### Content Quality

| Guideline | Description |
|-----------|-------------|
| **Concise** | 100-300 words per memory |
| **Structured** | Use consistent template |
| **Specific tags** | Precise classification |

### Tagging Strategy

```bash
# By technology
memo embed "..." --tags vue,frontend

# By importance  
memo embed "..." --tags important,decision

# By project
memo embed "..." --tags project-x,config

# By type
memo embed "..." --tags security,bug-fix
```

### Search Tips

| Strategy | When to Use |
|----------|-------------|
| Extract core concepts | Start with precise terms |
| Progressive search | Exact → fuzzy → broader |
| Lower threshold | When initial search finds nothing |
| Time filters | Recent work or specific period |

### Common Mistakes

| ❌ Don't | ✅ Do |
|----------|--------|
| Record entire code files | Extract key snippets only |
| Record every answer | Only valuable insights |
| Record without checking | Search first to avoid duplicates |
| Use vague titles | Be specific and descriptive |
| Too many generic tags | Keep tags focused |

---

## Usage Examples

### Record with Tags

```bash
memo embed "Rust error handling - Use anyhow for app-level code

背景：Application code needs simple error propagation
方案：Use anyhow::Result as return type, ? for propagation
关键点：Use thiserror for libs, anyhow for apps" --tags rust,error-handling
```

### Time-Based Search

```bash
# Recent work
memo search "database optimization" --after 2026-01-20

# Specific period
memo search "project progress" --after 2026-01-01 --before 2026-01-31 -n 20
```

### File with Tag Merging

If Markdown has frontmatter `tags: [rust, cli]`:

```bash
memo embed notes.md --tags important
# Final tags: rust, cli, important
```

## Trigger Phrases

| Action | Phrases |
|--------|---------|
| **Record** | "记住这个", "记录下来", "总结一下经验" |
| **Search** | "之前是怎么做的", "查一下记忆", "还记得吗", "最近有没有..." |
