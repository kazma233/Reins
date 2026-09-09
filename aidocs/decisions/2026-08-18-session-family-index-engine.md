# session family index 提取为共享引擎

- 日期：2026-08-18
- 状态：生效中（候选3 第一批；第二批 timeline 聚合引擎未开始）

## 背景

三个 session backend（`claude_code.rs` / `codex.rs` / `opencode.rs` 各 ~1450 行）各有一套近乎复制的 family index：Row/Family/CacheEntry 结构、全局 static 缓存与命中校验、members 与 families 排序、`sessions_by_path` 构建、双写 `sessions_by_id`。前置批次已统一双写语义（59b8ee2）并让全部 session 测试在本机可跑（46165c5），提取条件成熟。

## 决策

新模块 `session/family_index.rs` 引擎，形状：

1. **`FamilyRow` trait（六方法）承载 backend 差异**：`member_path()`（`Cow<Path>` 容纳 opencode 的按 id 派生路径）、`family_root_id()`（root 自己的 source id，非分组 key——孤儿 family 的 fallback root 只有自身 id 可认领 family entry）、`member_id()`（claude 为 `agent_session_id` 维度，codex 为 `source_session_id`）、`member_created_at/updated_at()`、`member_tie_breaker()`。backend 各自 ~35 行 impl 声明映射。
2. **分组策略留在 backend**：三家 genuinely 不同（claude 按 transcript 内共享 sessionId、codex 沿父链上溯、opencode SQL parent_id 关联），引擎从 `Vec<Family<Row>>` 开始接管。
3. **by_id 可关闭**：`FamilyIndex::build`（无 id map，opencode 用，其按 id 解析直接查 db）/ `build_with_ids`（双写 map）。**双写规则只存在这一份实现**：family key `or_insert` → root.path（first-claim-wins）+ member id → member.path。
4. **确定性**：members 先按 `(created_at, tie_breaker)` 排序、families 再按 `(updated_at 降序, root path)` 排序，之后才建 map——消除对 WalkDir 枚举顺序的依赖，索引是输入的纯函数。
5. 缓存 static、`lock_family_index_cache`、`find_session_file` fallback、`session_family_for_path` 错误消息留在 backend；缓存命中判断进 `FamilyIndexCacheEntry::is_valid`。

## 迁移中核验过的等价性

- opencode 原 families 排序（`root.time_updated + root.id`）与其他两家（族内 max updated_at + root path）不同，统一进引擎前确认 `list_session_families` 唯一消费方是 `list_entries`，输出经 `sort_entries`（timestamp desc + path 全序）重排——families 顺序零可观察出口。
- claude members 平局键由 Path cmp 改为 display 字符串 cmp：同族同目录文件名时逐字节等价。
- opencode `family_created_at/updated_at` 的 `unwrap_or(root.*)` 是死分支（root 恒为 members 成员），引擎 `unwrap_or_default()` 等价。
- codex `cached_family_summary` 的 `Some(unwrap_or_default)` 变 Option 直传：仅在全族无时间戳的不可达分支有差异。

## 生效约束

- 双写规则、path_key、确定性排序的修改只能动 `family_index.rs`，不得在 backend 复活局部实现。
- 新 backend 接入 = 实现 `FamilyRow` + 分组函数 + 自有 static 缓存。
- `tests/session_index.rs` 两个 resolve_path 冲突测试是双写语义的护栏，改引擎必须保持其通过。

## 验证

- `cargo fmt --check` / `cargo check` 干净（仅存量 ts-rs serde 提示）；全量 `cargo test` 103 通过 / 0 失败 / 3 忽略（基线 97 + 引擎新增 6：双写、member id 撞 family key、乱序确定性、by_id 关闭、families 排序、缓存校验）。
- 三个 backend 净 -77 行（1430→1406 / 1586→1545 / 1444→1432），引擎 337 行（约 120 行为测试）。

## 第二批（timeline 聚合引擎）的接缝备忘

已由 `2026-08-18-session-timeline-aggregation.md` 落地，本节为当时的分析存档：

- 三份同形的 `cached_messages_for_family` / `cached_events_for_family` 是最大重复块，差异仅在新鲜度计算（claude 父目录 mtime / codex 文件 mtime / opencode db mtime）→ `FamilyRow` 加 `timeline_freshness()`。
- “是否子代理”判定三家不统一（`is_root` 字段 / id 比较 / db id 比较）→ 引擎提供成员分类。
- `(+N subagents)` 标题逻辑三份同形可顺带收编；codex 独有的 root 事件扫描（`subagent_lifecycle_events`）需留钩子或留 backend。
