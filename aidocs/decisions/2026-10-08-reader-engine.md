# family 读取器迁入共享读取器引擎（reader_engine.rs）

- 日期：2026-10-08
- 状态：生效中
- 前置：`2026-10-08-session-source-registry.md`（来源注册表，与本文件同一评审批次）

## 背景

架构评审候选 2：7 个会话读取器各自复制约 250-350 行机械样板（缓存 static、锁助手、family 管道包装、分页五元组包装、SessionReader impl），且 static 单例 + 路径从 env 推导迫使全部 reader 测试共享一把全局锁并突变进程 HOME（TestEnvGuard），无法并行。逐 reader 解剖发现三个与「7 份机械复制」假设的偏差，决定了范围修正：

- **grokbuild 根本不落这套模板**（零缓存 static、events 流式分页是刻意的内存契约、family index 每次全量重建）——完全不迁。
- **pi 没有 family**（条目列表索引、无 summary 条目、双半缓存要求两半同时命中）——不进引擎，只复用部件。
- **dsh 一个 reader 两种失效口径**（index 用指纹串、timeline 用 mtime）+ **zstd 双解码缺陷**（messages/events 两个 loader 对同一多帧转录各完整解码一遍）。

interface 形状经 design-it-twice 三方案对比（极简数据表 / 16 钩子 trait + 组合部件 / 默认画像 trait）后定为第三方案骨架 + 两处吸收。

## 决策

1. **`session/reader_engine.rs` 是 family 机械学的唯一实现**：`FamilySpec` trait（20 方法、必填 8 个、默认值 = 典型画像：family + mtime 失效 + root 转录摘要 + timeline 计数）；`ReaderEngine` 实例持三层缓存（目录索引、双半时间线、三级摘要）；`FamilyReader` static 前置按解析出的 root 换实例（Arc 槽，换根即整实例重建）。
2. **行为契约由 label 数据逐字还原**：锁 poisoned 文案（含 dsh 的 "DSH index" 无 family 字样、claude 锁前缀 "Claude" 而错误文案 "Claude Code"）、not-found 文案、skip 日志格式，全部与迁移前逐字一致。
3. **失效口径值化**：`Freshness::Stamp(i64) | Fingerprint(String)`，engine 只做相等比较、不假设单调性——dsh 的双口径（index 指纹 / timeline mtime）由此表达且刻意保留。
4. **timeline 单 loader 双半**：`load_members` 每成员一次扫描/查询产出 (messages, events)，marker 注入、session_id 兜底、(timestamp, id) 终排由引擎统一执行——顺带修复 dsh 的 zstd 双解码（冷读一个 family 解码次数减半）。marker 外壳（正文/摘要/kind/基底 payload）由引擎组装，读取器只给 `MarkerShape`（id 方案 + message/event 两侧 extras，dsh 的不对称 payload 由此表达）。
5. **三派口径显式枚举**：`OverviewCounts`（Timeline 默认 / Omitted / Declared+引擎补 marker 数）、`SummaryKind`（RootTranscript 默认三级缓存 / Rows 行直构）、`RowErrorPolicy`（SkipSilently / SkipLogged / Abort）。
6. **迁移范围**：codex（8ff1bdf）→ claude（bebfed4）→ opencode+zcode（f1074ff）→ dsh（3bc80aa）全进引擎；pi（25b3eb6）只换部件（DualHalfCache、message_page/event_page；摘要缓存保持现状——它是最后一份拷贝且全局持久层的 HOME 重开语义要保留）；grokbuild 永不迁移；delete 留在各 reader、结尾调 `BACKEND.engine()?.clear()?`。
7. **持久缓存随实例注入**：summary_cache.rs 增实例级 `SummaryDb { dir }`；生产经 FamilyReader 解析 `~/.reins`（换根即换库），测试经 `engine_at(root, store_dir)` 传临时目录。**计划中的 v2→v3 版本号 bump 未执行**：核实发现持久层键本来就是 canonical path_key（display 串只存在于内存层）、解析语义未变，bump 只会让用户白吃一次冷缓存。
8. **family_timeline.rs 收窄为聚合层**：load-through 缓存机器（TimelineCacheEntry / cached_family_* / store_timeline_items）与 family_index.rs 的 FamilyIndexCacheEntry 删除——六家读取器全迁后无调用方；`apply_summary_aggregates` / `family_agents` / `agent_messages` / `sum_token_usage` / `FamilyAgentLabel` 保留，由引擎调用。

## 生效约束

- family 形态读取器的缓存生命周期、失效判定、marker 外壳、排序、分页、clear 编排只存在于 reader_engine.rs，不得在读取器复活局部实现。
- 分组与 root 选取（孤儿语义、resume 选根、is_root 判定）留在读取器——family_index.rs 头注释声明的边界不变。
- marker 的 id 方案与 payload 字段是前端契约，改动需过 session_readers 断言；锁/not-found 文案同理。
- 新增 family 来源 = 新 reader 文件 + `FamilySpec` 实现（典型来源只写必填 8 项）+ sources.rs 一条注册。
- `engine_at(root, store_dir)` 是 reader 级测试的构造入口：完全脱离进程 env、可并行。（2026-10-09 更新：存量 reader 级测试已迁移 engine_at，五家文件的 TestEnvGuard 用量 31→6；opencode/zcode 的数据访问已改接实例 scan_root——此前 engine_at 对这两家是假脱 env，红测试实测读到本机真实 db。）

## 验证

- 引擎单元测试 12 个（缓存命中跳过重扫、指纹失效重建、换根整实例重建、三派计数分派、marker 装配与 session_id 兜底、持久层跨实例命中、锁 poisoned 文案格式、坏行三策略），全部经 ReaderEngine 真实接口。
- 全量 `cargo test`：359 通过 / 0 失败 / 2 忽略（存量 reader 用例零改动通过 = 等价性主证据；6 个 family_timeline 缓存机器用例随机器删除）。
- `cargo fmt --check` / `cargo check --all-targets` 0 error 0 warning。
- 净行数：五家读取器 -1196 行（claude 945→653、codex 1187→860、opencode 1024→838、zcode 989→803、dsh 1116→961）+ pi -50；reader_engine.rs 新增 1393 行（约四成为测试）；family_timeline.rs 534→287。前端 `src/` 与 ts-rs 契约零变化。
