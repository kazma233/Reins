# Pi 会话来源接入 review 发现的问题

- 日期：2026-09-05
- 范围：未提交的 Pi 来源接入改动（`session/pi.rs`、`tests/pi.rs`、跨来源导入/删除/前端接线）
- 状态：P1/P2 五项已于同日修复（见各条目 commit）;`cargo test --lib` 150 通过 / 3 忽略（另有存量失败 `session_cache::persistence_survives_process_restart`,与本批无关,stash 验证过基线同样失败）、`cargo clippy` 对 pi.rs/mod.rs 无警告、`cargo fmt --check`、`pnpm test`、`pnpm build`(含 vue-tsc)通过;用本机 Pi 0.85.0 `SessionManager.list/open + buildSessionContext` 做过导出 round-trip 验证(自定义目录可见、上下文链完整、isError 保留、旧嵌套布局官方不可见)。

## 结论

`cargo check --all-targets`、Rust 全量测试（147 通过 / 3 忽略）、`pnpm test`、`pnpm build`、`cargo fmt --check` 均通过；`cargo clippy -D warnings` 失败（35 项，绝大部分为仓库存量问题，新增 `pi.rs` 的 `collapsible_if` 属本批引入）。功能可用但存在与 Pi 官方契约不符的数据兼容问题，按严重度分 P1/P2。

## P1(已修复,commit `767bcf5`)

### 1. 自定义 sessionDir 语义错误，Pi 发现不了导入结果 ✅

- 修复：`configured_session_dir()` 按官方优先级(env > settings.json)解析;配置目录时文件直接写在其根下,默认路径才叠加 cwd 编码目录;补读 `<agentDir>/settings.json` 的 `sessionDir`;`prune_empty_parents` 不再删除会话根目录本身(顺带修复:此前自定义目录/`sessions/` 被清空后会被整个删掉)。

### 2. v1 线性会话解析后只剩最后一条 entry ✅

- 修复：`read_document` 按官方 `migrateV1ToV2/V3` 在内存补齐 id/parentId 线性链(含 compaction `firstKeptEntryIndex` -> `firstKeptEntryId`、`hookMessage` -> `custom`),不写回磁盘。注意官方 v1 header 仍有 `id` 字段,缺 id 的文件官方也视为无效。

### 3. compaction 的 retainedTail 未恢复成消息 ✅

- 修复：`build_timeline` 遇到 compaction entry 时展开为 compactionSummary 消息 + retainedTail 逐条转换的消息。浏览器口径全量展示,不过滤 `stopReason` 为 error/aborted/deferred 的助手消息(官方仅在重建 LLM 上下文时过滤)。

## P2(已修复,commit `e24f34d`)

### 4. 工具失败状态丢失 ✅

- 修复：`ContentBlock` 增加 `is_error`(ts-rs 可选字段,前端契约向后兼容);Pi toolResult 读入 `isError` 并在导出时透传;bashExecution 的 `exitCode/cancelled/truncated` 从源消息 payload 透传,不再硬编码。其他来源(claude/codex/opencode)读侧暂未填 `is_error`,跨来源错误标记透传待后续按来源补。

### 5. 导出 entry 外层 timestamp 全用会话创建时间 ✅

- 修复:`ExportClock` 提取到 `session/mod.rs` 共享(codex 复用);Pi 导出外层 timestamp 跟随消息时间,同值/倒序时前进 1ms 保持单调。header 与 session_info 同为创建时刻。

## 新发现的相邻问题(未修复,待决策)

- **自定义目录下 `pi /resume` 列表按 cwd 过滤**：Pi 在自定义 sessionDir(≠默认路径)下运行时,`SessionManager.list(cwd, sessionDir)` 只显示 header `cwd` 匹配当前 cwd 的会话(`filterCwd` 逻辑)。agent-tools 导入的 header.cwd 是源会话 cwd,与用户在 Pi 里的 cwd 不同时项目级 resume 看不到(全局 listAll 能看到)。是否导入时改写 header.cwd 属产品决策。

## 代码质量与测试(部分完成)

- `pi.rs` 单文件超 1200 行,建议按契约拆分——未做,建议单独提交。
- 测试已补:settings.json.sessionDir 与 env 优先级、自定义目录布局(导入落根/删除保留根)、v1 线性会话、retainedTail、isError/bashExecution 状态 round-trip、外层 timestamp 跟随消息时间(含倒序单调)。
- Pi CLI round-trip(`pi --session` / `pi /resume` 实机验证)仍建议后续补,本次以官方 SessionManager 脚本验证代替。
