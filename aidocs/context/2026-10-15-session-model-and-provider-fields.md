# 会话模型与供应商的取值口径

- 日期：2026-10-15
- 性质：实现前后的字段核对记录。会话列表角标、详情页「模型」行、消息页每次输入/输出的模型标注都由本文的字段支撑；新增来源或上游格式变更时按本文重新核对。

## 各来源的字段与覆盖（本机实测）

| 来源 | 模型字段 | 供应商字段 | 粒度 | 本机覆盖 |
| --- | --- | --- | --- | --- |
| Codex | `turn_context.payload.model` | `session_meta.payload.model_provider` | provider 会话级、模型逐轮 | 706/714 会话（无 `turn_context` 的老会话为空） |
| Claude Code | `assistant.message.model` | **无** | assistant 行 | 有 assistant 行的文件才有 |
| OpenCode | `session_message.data.model.id`（`session_v2.model` 为回退） | `data.model.providerID` | assistant 消息 | 355/356 会话 |
| Pi | `message.model` | `message.provider` | assistant 消息 | 120/120 |
| Grok Build | `usage.json` 的 `turns[].modelUsage` 键（回退 `summary.json` 的 `current_model_id`） | **无** | 会话级 | 56/56 |
| ZCode | `model_usage.model_id` | `model_usage.provider_id` | 调用级，可经 `assistant_message_id` 关联到消息 | 44/44 |
| DSH | `request/context.model`（旧会话回退 `model/selection`） | `request/context.provider` | 每次请求 | 25/26 |

要点：

- **供应商不是统一命名**：值是各工具自己配置里的 provider id（本机实测有 `custom`、`reins-fanggeek`、`tokenflux`、`account:bigmodel-individual-coding-plan`、`new-provider` 等）。只有 Reins 自己写入的注册键（`reins-<provider.id>`）能反查到 providers.yaml 的显示名，其余按原样展示，不做猜测映射。
- **Claude Code 与 Grok Build 的转录里没有 provider 字段**：Claude 的全部行只带 `message.model`（`system` 行的字段是 version/cwd 一类），Grok 的 `usage.json`/`summary.json`/`events.jsonl`/`chat_history.jsonl` 全文无 `provider`。两者的供应商留空，不由当前配置反推历史会话。
- **ZCode 的调用级关联**：`model_usage.assistant_message_id` 与 `message.id` 实测命中率 99.7%，因此消息级归属可直接按 id 关联；同一条消息多次尝试时取最早一次。
- **DSH 的模型声明点**：`request/context` 出现在该轮用户消息之后、assistant 消息之前，本轮 assistant 消息与用量都归它；`model/selection` 只在切换时出现，作为旧格式回退。这条替换同时改变了用量统计的模型维度（原先只认 `model/selection`，多数会话没有该事件）。

## 归属规则

- **消息级**：assistant 消息直接带该次调用的模型；Codex/DSH 按轮继承（`turn_context` / `request/context` 声明之后到下一次声明之前）。用户输入本身不带模型，由这一轮的回答决定（同一次扫描里其后第一条带模型的消息），末尾没有后续回答时留空。
- **会话级**：会话内按调用次数排序，首项为主模型；同一模型换过 provider 时分开计项。
- **family 并集**：子代理线程/子会话可能换模型，与 token 求和的 family 口径一致按成员合并；跨成员的调用次数不可比，合并只做保序去重，root 的主模型仍是首项（Codex 本机实测有 4 成员 4 模型的会话）。
- **缓存**：模型存在 `SessionSummary` 里，摘要缓存键只看 mtime，字段变更必须 bump `summary_cache::DB_FILE_NAME`（模型这次 v3 → v4；标题不再截断又 v4 → v5）。

## 界面口径

- 列表角标：主模型 + `+N`（N 为其余模型数），完整清单放 `title`；没有模型信息的会话不占位。
  卡片附加信息在标题下一行，一行 flex 铺开：时间 / 来源名 / 模型角标 / 已归档 / 已压缩 / 使用量，
  放不下整项换行（`.session-meta-row`）。
- 来源名只在「全部」视图显示（`showAgent` 属性）：单一来源下每条都相同，是冗余信息。
- 使用量是「总消耗 + 缓存命中率」两段（`formatSessionUsage`，命中率口径见
  `token-usage.ts` 的 cacheHitRate），分项明细留在详情页。
- 详情页「模型」行：`provider/model` 顺序拼接，无 provider 时只显示模型名。
- 消息页：只标在输入与输出上，过程行（思考/工具/错误）不标；一条消息被过程行切成多段文本时只在首段标一次。

## 「只看对话」的作用点

过程行（思考/工具/错误）默认就已经是收起的一行，所以这个模式必须另有效果：
开启时把过程行合并成一行摘要（`groupProcessItems`，「过程 N 项（思考 x · 工具 y）」），
**单条过程也收进组**——留下裸过程行会让「只看对话」这个名字与结果不符；点开摘要
仍是原有过程行、可逐条展开；关闭时恢复逐条一行。筛选（搜索）期间不分组，否则命中
项会被收进组里，看起来像搜不到。分组只发生在渲染序列（SessionDetail 的
displayItems），标签计数与搜索仍用未分组的 `visibleItems`。

开关本身是工具栏里的勾选框（原生 checkbox，只在消息页出现），不是与「消息/raw」
同款的标签页样式——它不切换视图，只是收起过程行；raw 页没有过程行可收，放在那里
会变成点了没反应。
