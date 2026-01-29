# 命令参考

Memo CLI 所有命令的详细文档。

中文 | [English](../COMMANDS.md)

## 目录

- [`memo init`](#memo-init---初始化配置) - 初始化配置
- [`memo embed`](#memo-embed---嵌入记忆) - 嵌入文本/文件/目录
- [`memo search`](#memo-search---搜索记忆) - 语义搜索
- [`memo list`](#memo-list---列出记忆) - 列出所有记忆
- [`memo update`](#memo-update---更新记忆) - 更新已有记忆
- [`memo merge`](#memo-merge---合并记忆) - 合并多条记忆
- [`memo delete`](#memo-delete---删除记忆) - 删除记忆
- [`memo clear`](#memo-clear---清空数据库) - 清空数据库

---

## `memo init` - 初始化配置

初始化配置（可选，首次使用时会自动初始化）。

### 语法

```bash
memo init [OPTIONS]
```

### 选项

| 选项 | 说明 |
|------|------|
| `-l, --local` | 在当前目录初始化本地配置 |

### 示例

```bash
# 初始化全局配置（默认）
memo init

# 为当前项目初始化本地配置
memo init --local
```

---

## `memo embed` - 嵌入记忆

将文本、文件或目录嵌入到向量数据库。

**智能重复检测**：默认情况下，`embed` 会检查相似记忆，如果发现重复则取消操作。

### 语法

```bash
memo embed <input> [OPTIONS]
```

### 参数与选项

| 参数/选项 | 说明 | 默认值 |
|----------|------|--------|
| `<input>` | 文本字符串、文件路径或目录路径 | - |
| `-t, --tags` | 添加标签（逗号分隔，如：`rust,cli`） | - |
| `-f, --force` | 跳过重复检测，强制添加 | `false` |
| `--dup-threshold` | 重复检测的相似度阈值（0-1，覆盖配置文件） | `0.85` |
| `-l, --local` | 使用本地数据库 `./.memo/brain` | - |
| `-g, --global` | 使用全局数据库 `~/.memo/brain` | - |

### 示例

```bash
# 嵌入文本并添加标签
memo embed "重要笔记" --tags work,important

# 强制添加（跳过重复检测）
memo embed "相似但不同的内容" --force

# 自定义重复检测阈值
memo embed "内容" --dup-threshold 0.9

# 嵌入文件和目录
memo embed notes.md --tags rust,learning
memo embed ./docs --tags documentation
```

### 重复检测工作流

当检测到相似记忆时：

```
    Database ~/.memo/brain (16 records)

    Encoding text
    Checking for similar memories

     Warning Found 2 similar memories (threshold: 0.85)

[0.92] abc123-def456-789abc... (2026-01-20 10:30)
       Rust async trait - 使用 async-trait crate
       背景：trait 中直接使用 async fn 会导致编译错误

[0.88] def456-789abc-012def... (2026-01-19 15:20)
       另一个相关的 async 模式...

        Note Use --force to add anyway, or update/merge/delete existing memories
```

**建议操作：**
- 强制添加：`memo embed "..." --force`
- 更新记忆：`memo update <id> --content "..."`
- 合并记忆：`memo merge <id1> <id2> --content "..."`
- 删除记忆：`memo delete <id>`

### Markdown 文件标签合并

Markdown 文件的 frontmatter 标签会与命令行标签自动合并：

```markdown
---
tags: [rust, cli]
---
```

运行 `memo embed file.md --tags important` → 最终标签：`[rust, cli, important]`

---

## `memo search` - 搜索记忆

使用语义相似度搜索和探索相关记忆。

### 语法

```bash
memo search <query> [OPTIONS]
```

### 参数与选项

| 参数/选项 | 说明 | 默认值 |
|----------|------|--------|
| `<query>` | 搜索查询字符串 | - |
| `-n, --limit` | 返回的最大总节点数 | 5 |
| `-t, --threshold` | 相似度阈值（0-1） | 0.7 |
| `--after` | 时间范围：之后 | - |
| `--before` | 时间范围：之前 | - |
| `-l, --local` | 使用本地数据库 | - |
| `-g, --global` | 使用全局数据库 | - |

### 时间格式

- `YYYY-MM-DD` - 例如：`2026-01-20` (00:00)
- `YYYY-MM-DD HH:MM` - 例如：`2026-01-20 14:30`

### 工作原理

- 返回层次化的相关记忆结构
- 第 1 层：与查询直接匹配的记忆
- 第 2+ 层：使用父记忆作为种子查找的相关记忆
- 使用自适应阈值和标签过滤确保相关性
- 每条记忆只出现一次（去重）
- 支持使用 `--after` 和 `--before` 进行时间过滤
- `-n/--limit` 控制所有层级的最大总结果数

### 示例

```bash
# 基本搜索
memo search "Rust 最佳实践"

# 自定义参数搜索
memo search "Vue 技巧" --limit 10 --threshold 0.6

# 基于时间的搜索
memo search "开发经验" --after 2026-01-20
memo search "会议记录" --after "2026-01-20 09:00" --before "2026-01-20 18:00"

# 搜索更多结果
memo search "异步模式" -n 20
memo search "错误处理" --threshold 0.65 -n 30
```

### 输出示例

搜索会以层次化格式显示完整的记忆内容，包括 ID、日期、标签和全文。使用 `[LAYER1]`、`[LAYER2]` 等标签标识层级关系：

```
[LAYER1] [0.85] a1b2c3d4-e5f6-7890-abcd-ef1234567890 (2026-01-27 10:30) [rust, async, trait]
                Rust 异步模式 - async-trait 使用指南
                
                背景：在 trait 中直接使用 async fn 会导致编译错误
                方案：使用 #[async_trait] 宏修饰 trait 定义和实现
                关键点：需要在 trait 和 impl 块都添加该宏
                
       [LAYER2] [0.78] b2c3d4e5-f6a7-8901-bcde-f12345678901 (2026-01-26 14:20) [rust, async, error]
                       异步错误处理 - Result<T, E> 使用
                       
                       背景：异步函数中需要优雅地处理错误
                       方案：返回 Result<T, Box<dyn Error>> 或使用 anyhow::Result
                       关键点：使用 ? 操作符进行错误传播
       
       [LAYER2] [0.75] c3d4e5f6-a7b8-9012-cdef-123456789012 (2026-01-25 09:15) [rust, tokio, runtime]
                       Tokio 运行时配置 - 多线程 vs 单线程
                       
                       背景：选择合适的 Tokio 运行时模式
                       方案：多线程用 #[tokio::main]，单线程用 current_thread
                       关键点：根据应用负载特性选择

[LAYER1] [0.82] f9a8b7c6-d5e4-3210-fedc-ba9876543210 (2026-01-26 15:45) [rust, error]
                Rust 错误处理最佳实践
                
                背景：应用层和库层需要不同的错误处理策略
                方案：应用层用 anyhow，库层用 thiserror
                关键点：避免在库中使用 anyhow
```

---

## `memo list` - 列出记忆

列出数据库中的所有记忆（按更新时间排序）。

### 语法

```bash
memo list [OPTIONS]
```

### 选项

| 选项 | 说明 |
|------|------|
| `-l, --local` | 使用本地数据库 |
| `-g, --global` | 使用全局数据库 |

---

## `memo update` - 更新记忆

更新已有记忆的内容和标签。

### 语法

```bash
memo update <id> [OPTIONS]
```

### 参数与选项

| 参数/选项 | 说明 |
|----------|------|
| `<id>` | 要更新的记忆 ID |
| `-c, --content` | 新内容（必填） |
| `-t, --tags` | 新标签（逗号分隔，会替换现有标签） |
| `-l, --local` | 使用本地数据库 |
| `-g, --global` | 使用全局数据库 |

### 示例

```bash
# 只更新内容
memo update abc123 --content "更新后的内容"

# 同时更新内容和标签
memo update abc123 --content "新内容" --tags rust,updated,important
```

**注意：** 更新会重新生成嵌入向量，但保留原始的 `created_at` 时间戳。

---

## `memo merge` - 合并记忆

将多条记忆合并为一条。

### 语法

```bash
memo merge <ids>... [OPTIONS]
```

### 参数与选项

| 参数/选项 | 说明 |
|----------|------|
| `<ids>...` | 要合并的记忆 ID（空格分隔） |
| `-c, --content` | 合并后的内容（必填） |
| `-t, --tags` | 合并后的标签（如不指定则自动合并所有标签） |
| `-l, --local` | 使用本地数据库 |
| `-g, --global` | 使用全局数据库 |

### 示例

```bash
# 使用自定义内容合并
memo merge id1 id2 id3 --content "关于...的整合知识"

# 使用自定义内容和标签合并
memo merge id1 id2 --content "合并内容" --tags rust,summary

# 合并（标签自动合并）
memo merge id1 id2 id3 --content "综合见解"
```

**注意：** 合并后的记忆会保留原始记忆中最早的 `created_at` 时间戳。

---

## `memo delete` - 删除记忆

根据 ID 删除记忆。

### 语法

```bash
memo delete <id> [OPTIONS]
```

### 参数与选项

| 参数/选项 | 说明 |
|----------|------|
| `<id>` | 要删除的记忆 ID |
| `-f, --force` | 跳过确认提示 |
| `-l, --local` | 使用本地数据库 |
| `-g, --global` | 使用全局数据库 |

### 示例

```bash
memo delete abc123
memo delete abc123 --force
memo delete xyz789 --local
```

**注意：** 除非指定 `--force`，否则会提示输入 `yes` 确认删除。

---

## `memo clear` - 清空数据库

⚠️ **危险操作**：清空指定数据库中的所有记忆。

### 语法

```bash
memo clear [OPTIONS]
```

### 选项

| 选项 | 说明 |
|------|------|
| `-l, --local` | 清空本地数据库 |
| `-g, --global` | 清空全局数据库 |
| `-f, --force` | 跳过确认提示（谨慎使用） |

---

## 通用选项

这些选项在多个命令中可用：

| 选项 | 说明 |
|------|------|
| `-l, --local` | 使用本地数据库（`./.memo/brain`） |
| `-g, --global` | 使用全局数据库（`~/.memo/brain`） |
| `-t, --tags` | 添加/设置标签（逗号分隔） |
| `-f, --force` | 跳过确认提示 |

## 使用技巧

### 标签策略

```bash
# 按技术栈分类
memo embed "Vue 技巧" --tags vue,frontend

# 按重要性分类
memo embed "关键决策" --tags important,decision

# 按项目分类
memo embed "项目文档" --tags project-x,docs

# 组合多个分类
memo embed "安全修复" --tags security,bug-fix,important
```

### 时间过滤场景

```bash
# 查看最近的记忆
memo search "开发经验" --after 2026-01-20

# 查看某个时间段的工作记录
memo search "项目进展" --after 2026-01-01 --before 2026-01-31

# 查看今天的记录
memo search "会议" --after 2026-01-25
```

### 多项目管理

```bash
# 项目 A：使用本地数据库
cd /path/to/project-a
memo embed ./docs --local --tags project-a

# 项目 B：使用独立配置
cd /path/to/project-b
memo init --local  # 创建 ./.memo/config.toml
memo embed ./docs --tags project-b
```
