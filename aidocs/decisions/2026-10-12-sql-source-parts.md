# SQL 来源共享部件：父链分组、用量口径与 tool 块装配

- 日期：2026-10-12
- 状态：生效中

## 背景

opencode 与 zcode 的 SQL 读取器存在四份拷贝级重复（zcode 接入时直接拷了 opencode 的实现），codex 另有一份同形变体：

- `build_session_families` + `root_session_id`（含环检测）：zcode ≡ opencode 逐 token 相同；codex 同形但孤儿语义不同（父缺失保留最后已知 id，opencode/zcode 认领缺失父 id）。
- `tool_blocks` + `tool_input_text`：zcode ≡ opencode 几乎逐字相同，差异仅 4 处（工具名字段 `tool`/`name`、call id 字段 `callID`/`id`、输出提取 `state.output` 字符串 / `state.content` 块数组、payload 剥离键）。
- `usage_hours` 尾部循环（桶合并 + 两级会话数）与 `session_token_usages` 的行装配：同形，SQL 不同。
- 归一口径算术（input 减 cache_read、output 加 reasoning）每个文件写两遍，全仓四处——漏改即静默数字错误（无报错）。

`2026-08-18-session-family-index-engine.md` 与 family_index 头注释声称三家分组策略 "genuinely differ"，对 opencode/zcode 这对已不成立。同期的 Windows 修复（claude/grokbuild delete plan 的 `/` 字符串手术）也印证了 SQL/展示字符串上的重复手术是 bug 温床。

## 决策

1. **三件部件各回其家，不建新的大模块**：
   - 父链分组（`ParentChainRow` 契约、`OrphanPolicy`、`root_id_by_parent_chain`、`group_families_by_parent_chain`、`member_root`）进 `family_index.rs`——`Family<R>` 与行契约本就在那里。
   - 用量口径（`UsageColumns`、`split_reasoning_usage`、`full_scope_usage`、`hours_from_usage_rows`）进 `usage_stats.rs`——`SqlUsageHours` 与归桶机器本就在那里。
   - tool 块装配（`ToolShape` + `tool_blocks`）新建 `tool_blocks.rs`——装配体单一，两来源各声明一张形态表。
2. **孤儿策略显式化**：`OrphanPolicy::{ClaimMissingParent, KeepLastKnown}`，opencode/zcode 用前者、codex 用后者；root 挑选作函数参数（opencode/zcode 共用 `member_root`，codex 保留续跑段年龄的确定性挑选 `segment_age_root`）。三种行为逐字保留，family_chain 测试逐策略钉住。
3. **DSH 不纳入父链部件**：其父子边来自父日志 catalog 的声明（child→parent 边表）而非行内 parent_id 列，链断即孤儿即隐藏（返回 Option），数据结构与语义都不同，强行统一是把两种形态塞一个接口。
4. **输入列语义差异显式化为两个口径入口**（实现中被测试抓出的真差异）：zcode 的 `input_tokens` 是全口径（未命中 + 缓存读），须拆缓存读（`full_scope_usage`）；opencode 的 `$.tokens.input` 已不含缓存读，不得再拆（`split_reasoning_usage`）。共享的口径内核是「output 并入 reasoning + 负值钳 0 + input 必须不含命中」。
5. **SQL 与 None 判定留在各来源**：db 缺失/表缺失的不可用判定、SQL 文本、行解析、FamilySpec 声明均不动——加深部件，不再造引擎。
6. **usage_hours 孪生测试不强行收敛**：两家建的是不同表（`session_message` vs `turn_usage`）、不同时间语义（消息时间 vs turn 开始时间），共享的只有外壳，参数化只会造出弱抽象。

## 对既有文档的修订

- `2026-08-18-session-family-index-engine.md`：「三家分组策略 genuinely differ」的表述对本对已过时；family_index 头注释已改写为「行形态真不同的来源各自分组（claude、dsh），SQL parent_id 链三家共享分组骨架，只声明孤儿策略与 root 挑选」。引擎其余决策不受影响。

## 验证

- 新增部件单测：family_chain（孤儿两策略 + 环 + root 挑选）、tool_blocks（两形态 + running + 非文本回退）、口径双入口。
- 全量 `cargo test`：418 通过 / 0 失败 / 2 忽略，零警告；opencode/zcode/codex 的既有行为测试（含 resume 段 root 挑选、usage 小时桶断言）不经修改全绿，作为逐字等价证明。
