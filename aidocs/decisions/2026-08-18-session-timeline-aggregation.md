# session timeline 聚合层提取为共享模块

- 日期：2026-08-18
- 状态：生效中（候选3 第二批，随第一批 `2026-08-18-session-family-index-engine.md` 之后）

## 背景

family index 引擎落地后，三个 backend 下游仍有三份重复：`cached_messages_for_family` / `cached_events_for_family` 六个函数逐字相同（仅差 cache static、label、cache key 来源、新鲜度计算、loader）；claude/codex 的 `cached_family_summary` "(+N subagents)" 后处理与 `family_source_paths` 同形；`family_agents` 三家形状相似但 label 策略不同。

## 决策

新模块 `session/family_timeline.rs`（与 `family_index.rs` 分层：index = 族的结构与查找，timeline = 族的输出聚合，后者依赖 session model 类型，不污染前者）：

1. **`cached_family_messages` / `cached_family_events`**：泛型核心 `cached_family_items<T>` 吞 load-through 形状，backend 只传四个真差异（static、label、cache key、新鲜度 i64）+ loader 闭包。`TimelineCacheEntry`（原 model.rs）与 `cached_timeline_items` / `store_timeline_items`（原 timeline.rs）顺脚迁入，经 mod.rs re-export 保持 `super::` 引用不变。cache 以 `&Mutex<HashMap<...>>` 参数注入，测试无需 static。
2. **`Family::apply_summary_aggregates(&mut summary)`**："(+N subagents)" 标题、root transcript path、族级时间戳聚合一次实现；root 摘要来源（`cached_path_summary`）留 backend。opencode 的 `family_summary` 从 db row 直构，不经此形状。
3. **`Family::source_paths()`**（family_index.rs）：成员路径列表；opencode 自行 prepend db 路径（成员行由 db join 出，导入/删除动 db 不动 transcript）。
4. **`family_agents` + `FamilyAgentLabel`**：backend 闭包给出 `Root | Child(name) | Derived(name)`，roster 形状与 "主 Agent" / "(子)" / "(派生)" 文案统一在此，`is_root` 由 variant 推导（与 label 永不矛盾）。root 判定策略留 backend 闭包：claude 必须用 `row.is_root` 字段（非 agent-* 命名的 subagent 文件 member id 会退化为 family key，id 比较无法区分）；codex/opencode 用 id 比较。
5. **留 backend**：新鲜度计算（claude 叠加父目录 mtime / codex 文件 mtime / opencode db mtime）、`load_*_for_family` 与 marker 注入（对外契约）、codex 独有 `subagent_lifecycle_events`。

## 迁移核验过的等价性

- 三家 `family_agents` 原 session_id 来源（claude `agent_session_id`、codex/opencode id）均等于各自 `member_id()`。
- `family.root.path.display()` → `member_path().display()`：claude/codex 的 `member_path` 即 `Cow::Borrowed(&self.path)`。
- 锁中毒错误消息携带的 label（"Claude timeline" 等）原文保留，有测试断言传递路径。

## 生效约束

- timeline load-through 缓存、summary 聚合、roster 文案的新改动进 `family_timeline.rs`，不得在 backend 复活局部实现。
- marker 文案与 payload 是前端契约，改动需过 `session_readers` 断言。

## 验证

- `cargo fmt --check` / `cargo check --all-targets` 干净（仅存量 ts-rs 提示；subagent 漏跑 --all-targets 时主会话抓到两个测试代码警告已修）。
- 全量 `cargo test` 112 通过 / 0 失败 / 3 忽略（基线 103 + 新增 9：缓存命中/未命中/过期覆盖/messages-events 共享 entry/锁中毒 label/summary 两分支/roster 变体/source_paths）。
- backend 净 -153 行（claude 1406→1354、codex 1545→1485、opencode 1432→1391），family_timeline.rs 470 行（约半数为测试）。
