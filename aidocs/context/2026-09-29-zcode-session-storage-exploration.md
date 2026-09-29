# ZCode 会话存储探索与接入方案

- 日期:2026-09-29
- 性质:探索记录。为历史会话功能新增 zcode 来源提供的一手实测证据(zcode 3.14.4 桌面版本机数据)、官方文档与 CLI bundle 实证结论,以及据此确定的接入方案。实现前的决策依据,实现落地后由代码与测试取代。
- 探索环境:macOS,ZCode.app 3.14.4(`ZCODE_APP_VERSION` 实测),会话库 13 sessions / 1348 messages / 4952 parts。

## 结论(TLDR)

1. zcode 会话主存储是单个 SQLite:`~/.zcode/cli/db/db.sqlite`(WAL 模式),桌面版会话也写入此库(实测当前活跃会话即时可见)。
2. 存储形态与已支持的 OpenCode v2 高度同构(单库、`parent_id` 子会话链、JSON 消息体、工具块结构几乎一致),`session/opencode.rs` 是实现模板。
3. **没有自定义 home 机制**:官方文档无任何数据目录重定向说明;CLI bundle 里 db/rollout/log 路径均硬编码 `homedir()/.zcode/...`。发现逻辑固定读默认 home 即可。
4. 已定决策:不支持删除(同 Grok Build);token 用量取 `turn_usage` 预聚合表(优于 OpenCode 的 json_extract 方案)。

## 目录布局(`~/.zcode/`)

| 路径 | 内容 | Reins 是否需要 |
| --- | --- | --- |
| `cli/db/db.sqlite` | 会话主存储(本文重点) | 是 |
| `cli/config.json` | MCP 配置(workspace 域已接入,prefix `mcp.servers`) | 已有 |
| `cli/agents/sess_*/agent_*` | subagent 运行目录 | 首版不需要 |
| `cli/rollout/model-io-sess_*.jsonl` | 原始模型 IO 转录 | 首版不需要 |
| `cli/exec/` | 终端命令输出缓存(FAQ Q13,可安全删除) | 否 |
| `skills/` | skills 目录(workspace 域已接入) | 已有 |
| `v2/` | 桌面端数据(config.json、credentials.json、bot-state) | 否,**credentials 属敏感边界** |
| `AGENTS.md` | 全局指令文件 | 否 |

## 数据库实测

### session 表(会话元数据)

关键列:`id`(`sess_<uuid>`)、`title`、`directory`(cwd)、`parent_id`、`task_type`(`interactive` / `subagent_child`)、`time_created` / `time_updated`(毫秒,NOT NULL)。另有 `project_id`、`workspace_id`、`title_source`、`trace_id` 等,接入不需要。

subagent 判定:`parent_id IS NOT NULL`(子会话 task_type 为 `subagent_child`,id 形如 `sess_subagent_agent_<uuid>`),family 聚合沿 `parent_id` 链,与 OpenCode `session_v2` 相同。

### message 表 + semantics 分类边界

`data` JSON 的 `role` × `semantics.kind` 实测分布(共 4 种组合):

| role | semantics.kind | 数量 | 归类 |
| --- | --- | --- | --- |
| assistant | `assistant_response` | 1146 | 消息时间线 |
| user | `user_prompt` | 104 | 消息时间线 |
| user | `todo_reminder` | 99 | 事件(系统注入的 todo 快照) |
| assistant | `timeline_event` | 10 | 事件(model_change 分隔线等) |

这是现成的消息/事件二分边界,不需要按内容猜。消息文本不在 `message.data` 里而在 `part` 表;`data.metadata.inputIntent.text` 是用户输入副本(兜底可用,实测 user_prompt 消息均有 part,不需要)。

`data` 其他有用字段:`time.created`(毫秒)、`tokens`(单条增量,口径含 `cache.read/write`)、`contextSnapshot.envInfo`(user 首条消息携带 `gitBranch`、`gitStatus` 等,可作 cwd/git branch 来源)、`modelId` / `providerId`(assistant)。

### part 表(消息内容块,按 message_id 关联)

`type` 实测分布与结构:

| type | 结构要点 | 映射 |
| --- | --- | --- |
| `tool` | `{type, callID, tool, state:{status, input, output}}`,output 为字符串 | `function_call` + `function_call_output` 两块(同 OpenCode v2 口径) |
| `text` | `{type, text}` | `text` |
| `reasoning` | `{type, text}` | `thinking` |
| `file` | `{type, mime, url: zcode-artifact://...}` | `file`(显示 mime,不解析 artifact) |
| `step-start` | `{type}`,空标记 | 忽略 |
| `step-finish` | `{type, reason, cost, tokens:{...}}` | 忽略(量大,纯统计噪音) |
| `timeline` | `{timelineType, display, ...}`,挂在 timeline_event 消息下 | 事件 payload |

part 与消息的挂载关系(实测 GROUP BY):`user_prompt` 消息挂 `text` + `file`;`assistant_response` 消息挂 `tool` / `text` / `reasoning` / `step-*`;`todo_reminder` 消息挂 `text`;`timeline_event` 消息挂 `timeline`。

### turn_usage / model_usage 表(用量预聚合)

`turn_usage` 按 `(session_id, turn_id)` 主键,列直接给出 `input_tokens`、`output_tokens`、`reasoning_tokens`、`cache_creation_input_tokens`、`cache_read_input_tokens`。实测 `computed_total_tokens = input + output`,即 input 列不含缓存命中,与 Reins `SessionTokenUsage` 四项口径(input / output(含 reasoning) / cache_read / cache_write)天然对齐,一条 `GROUP BY session_id` SUM 即完成归一,不需要 json_extract。`model_usage` 是更细的请求粒度,接入用不到。

## 自定义 home 调研(文档 + 二进制实证)

官方文档(zcode.z.ai/en/docs,已翻 welcome 全导航 + install / configuration / qa / usage-stats):

- 无任何页面提及数据目录重定向或 `ZCODE_HOME` 类环境变量;FAQ Q12/Q13 列出的路径全是 `~/.zcode` 固定布局。
- 唯一提到的环境变量是 `HTTP_PROXY/HTTPS_PROXY`(默认忽略),与存储无关。

CLI bundle(`/Applications/ZCode.app/Contents/Resources/glm/zcode.cjs`,14MB)字符串实证:

- db 路径解析函数为 `join(homedir(), ".zcode", "cli", "db", "db.sqlite")`,无环境变量分支;`rollout`、`log` 目录同样硬编码。
- `ZCODE_STORAGE_DIR` 存在但只重定向 `cli/exec`(终端输出缓存),不影响 db。
- `ZCODE_HOME` 仅 1 处引用,只用于 telemetry device id,不是全局 home。
- `XDG_DATA_HOME` 引用来自第三方库(Vercel OIDC)通用逻辑,与 zcode 数据目录无关。

与 Reins 契约的一致性:README 口径本就是"历史会话发现固定读各工具默认 home"(Codex/Claude Code/OpenCode 同样如此),zcode 无需环境变量逻辑。

## 接入方案(已确认)

照 `session/opencode.rs` 骨架新建 `session/zcode.rs`:

- **后端**:`SourceApp::Zcode`(serde 名 `zcode`);reader 实现 `FamilyRow` + `SessionReader`;路径 key 用 `{db路径}:{session_id}` 组合串(同 OpenCode);family 沿 parent_id 链聚合,subagent 用 marker 消息/事件;`detect_sources_inner` / `available_sources` 以 `~/.zcode/cli/db/db.sqlite` 存在判定可用。
- **只读连接**:db 属主是常驻 ZCode 进程,用 `SQLITE_OPEN_READ_ONLY` 打开(WAL 支持并发读,实测 mode=ro 无锁)。
- **不支持删除**:`delete_session` bail,前端 `canDeleteSession` 排除(同 Grok Build)。
- **用量**:turn_usage 表 SUM;git branch 从首条 user 消息 `contextSnapshot.envInfo.gitBranch` 提取(可选)。
- **前端**:`pnpm codegen` 重生成 `SourceApp.ts`;`source-app.ts` label;`model.ts` 删除文案条目。
- **时间戳**:全库毫秒,直接进 `SessionSummary`。

## 遗留观察(实现时注意)

- `schema_migration` 表存在,数据格式有版本概念;解析对未知 `part.type` / `semantics.kind` 走现有 `unsupported_block` 降级路径。
- zcode CLI 不在 PATH(桌面版场景),无法像 OpenCode 那样调官方删除命令——这也是"不支持删除"决策的直接原因。
- step-finish part 携带每步 tokens,若未来需要更细粒度用量可作补充来源。
