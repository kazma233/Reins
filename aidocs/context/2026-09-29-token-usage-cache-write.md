# 会话 token 统计口径与缓存读写语义

- 日期：2026-09-29
- 性质：业务口径沉淀。会话列表 token 统计（输入/输出/缓存）上线时，围绕"缓存写是否存在、输入输出是否写反"的多轮质疑与验证结论，全部经本机真实数据或源码证据确认。
- 结论速览：缓存读/写都是真实概念（Anthropic prompt caching 字段），OpenAI 系协议没有写、数值恒为 0；`输入 = 未命中缓存的新输入` 与 pi / magpie / ccusage / Claude Code /cost 口径一致；reins 的入/出映射方向经 pi 0.87.1 源码验证没有反。

## 展示口径（最终决策）

列表条目三个指标，不拆缓存读写：

- **输入** = 未命中缓存的新输入 token（`SessionTokenUsage.inputTokens`）
- **输出** = 模型生成 token，含 reasoning/思考
- **缓存** = 缓存读 + 缓存写合计

数据模型保留四项（读/写分开存，`src-tauri/src/session/model.rs` 的 `SessionTokenUsage`），只是展示层合并；未来算成本或拆分展示时数据现成。

## Prompt caching 的读写语义

每轮请求都要重发完整上下文（系统提示 + 工具定义 + 全部历史），服务商允许把算过的部分存进缓存复用。token 按三种流向记账：

| 流向 | 发生时机 | Anthropic 计费 |
| --- | --- | --- |
| 输入（input） | 未走缓存、每次真实计算的新增 token | 1x |
| 缓存写（cache creation） | 上下文第一次进入缓存的那部分 | 1.25x（占缓存存储，约 5 分钟 TTL） |
| 缓存读（cache read） | 后续轮次命中缓存的相同上下文 | 0.1x |

三轮推演（系统提示+工具 15,000，每轮新增约 500）：

```
第 1 轮  上下文全新 15,500：缓存写 15,000 + 输入 500
第 2 轮  旧 15,000 命中 → 缓存读 15,000；新增 1,100 首次出现 → 缓存写 1,100
第 3 轮  旧 16,600 命中 → 缓存读 16,600；新增部分 → 又一次缓存写 ……
```

两个易错点：

1. **缓存写不是一次性的**。会话每增长一段（上轮回答 + 新消息），这段第一次出现就记一次写。
2. **写贵 25% 换来读只要 1/10 价格**，两轮回本；长会话里缓存读滚到百万级是常态（实测本机一个 Codex 会话：输入 7.2 万、缓存读 250 万），"输入比缓存小一个数量级"是正常形态，不是统计反了。

**为什么部分来源写恒为 0**：cache write 是 Anthropic 协议特有字段（`cache_creation_input_tokens`）。OpenAI 系协议只有 `cached_tokens`（读命中），首次发送按普通输入计费、无"写"概念。走 DeepSeek 等模型的会话写永远是 0；走 Anthropic 系网关的会话（Claude Code / OpenCode / Pi 常见）写非零——本机 OpenCode 库 3,767 条 assistant 消息 `cache.write > 0`。

## 各来源字段映射与归一

归一原则：对齐 Anthropic 口径（input 不含缓存读、output 含 reasoning）。

| 来源 | 原生字段位置 | input 处理 | output 处理 |
| --- | --- | --- | --- |
| Claude Code | 每条 assistant 行 `message.usage` | 原样（本就不含缓存读） | 原样（含思考） |
| Codex | `token_count` 事件的 `info.total_token_usage`（**累计值，只认最后一条**） | **减去** `cached_input_tokens`（OpenAI 口径 input 含缓存命中，实测 input+output==total） | 原样（output_tokens 已含 reasoning） |
| Pi | assistant `message.usage` + 子代理 run usage | 原样 | 原样（官方 totalTokens=input+output+cacheRead+cacheWrite，证明 output 已含 reasoning） |
| OpenCode | `session_message.data.tokens`（SQL 按会话 GROUP BY） | 原样 | **加上** `reasoning`（分列存储，实测 output 145 < reasoning 152） |
| GrokBuild | 会话目录 `usage.json` 的 `session` 汇总（各轮 turn_completed 累计，旧版本会话无此文件） | **减去** `cachedReadTokens`（同 Codex，实测 input+output==totalTokens） | 原样（已含 reasoning） |

Codex 与 OpenCode 的减法/加法是必要的归一，不是可选项：不减则"输入"与"缓存"双计同一批 token；不加则 OpenCode 输出少算 reasoning。

SQLite 注意：`SUM(a) + SUM(b)` 在任一列为 NULL 时整体为 NULL（`20 + NULL = NULL`），必须逐列 `COALESCE(SUM(...), 0)`，否则缺 `reasoning` 字段的行会把整个输出求和毒化为 0（夹具测试先抓出过这个问题）。

## 聚合口径

- 会话条目按 family 全体成员求和：root + resume 段 + 子代理（Claude 的 `subagents/` 文件、Codex 的 parent_thread 链、OpenCode 的 parent_id 链、GrokBuild 的子会话目录、Pi 内嵌的子代理 run）。
- Codex 的 `total_token_usage` 是文件级累计值，跨文件（resume/子代理）求和才是会话总量；单文件内多条 token_count 只取最后一条。

## 证据记录

| 主张 | 证据 |
| --- | --- |
| pi 状态栏 ↑=input、↓=output、R=cacheRead、W=cacheWrite | pi 0.87.1 源码（`@earendil-works/pi-coding-agent` bundle）：`usageTotals.input&&statsParts.push(\`↑${formatTokens(usageTotals.input)}\`)`；网传"↑=模型输出"图例与源码不符，pi 官方 docs 全目录搜不到该图例 |
| magpie 口径同款四分法 | magpie.app 二进制逆向：归一 `{input, output, cache_read, cache_write, cost}`，且 `prompt = tot.input + tot.cache_read`（input 不含缓存读）；统计方式为"Counted from the providers' own usage reports on every call through the gateway" |
| ccusage 四分类 | README 原文 "Tracks and displays cache creation and cache read tokens separately" |
| Codex input 含缓存命中 | 本机 10 个会话 `input_tokens >= cached_input_tokens`（如 71.9M input 中 71.6M 是 cached），且 input+output==total |
| OpenCode output 不含 reasoning | 本机样本 `output: 145, reasoning: 152`，output < reasoning 证明分列 |
| Grok usage.json 口径 | 本机会话 `input 41,331,412 + output 145,895 == totalTokens 41,477,307`；input > cachedRead 41,182,208 |
| reins 数值正确性 | 本机 Pi 会话 `01a0ebf2`（2026-09-29）独立复算 input=149,443 / output=8,486 / cacheRead=1,262,720，与列表显示及 pi 官方 ↑149k ↓8.5k、magpie 显示互相吻合 |
| GrokBuild 读取不炸 | 对本机 87 个真实会话目录跑 `grokbuild_local_store_readonly_acceptance` 通过（72 个含 usage.json） |

## 相关代码

- 数据模型与合并：`src-tauri/src/session/model.rs`（`SessionTokenUsage`）、`src-tauri/src/session/catalog.rs`（`merge_token_usage`、`build_summary`）
- family 求和：`src-tauri/src/session/family_timeline.rs`（`Family::sum_token_usage`）
- 各读取器聚合：`claude_code.rs` / `codex.rs` / `pi.rs` / `opencode.rs` / `grokbuild.rs`
- 前端展示：`src/features/sessions/components/SessionList.vue`（三指标 + tooltip）、`src/shared/lib/format.ts`（`formatTokenCount`）
- 测试：`src-tauri/src/tests/session_token_usage.rs`（五个来源的口径夹具）
