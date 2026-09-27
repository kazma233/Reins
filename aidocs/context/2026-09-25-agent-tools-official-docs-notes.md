# 五个 agent 工具官方文档要点记录（providers 之外）

- 日期：2026-09-25
- 性质：官方文档摘录笔记。记录 providers 核对之外、对 Reins 其他域（workspace/target、MCP 分发、skills 同步、会话读取）有直接影响的官方事实，供后续设计与排障引用。
- 来源均为 2026-09-25 抓取的官方页面；关键页加 `.md` 可取 Markdown（Claude/Codex 均支持）。

## 来源索引

| 工具 | 已读页面 |
| --- | --- |
| Grok Build | https://docs.x.ai/build/overview 、https://docs.x.ai/build/settings/reference |
| Claude Code | https://code.claude.com/docs/en/settings 、…/en/settings-reference 、…/en/env-vars 、…/en/model-config |
| OpenCode | https://opencode.ai/v2/docs/config/ 、…/docs/providers/ 、…/docs/migrate-v1/ |
| Pi | https://pi.dev/docs/latest/models 、…/settings 、…/custom-provider |
| Codex | https://learn.chatgpt.com/docs/config-file/config-reference 、https://learn.chatgpt.com/llms.txt |

## Grok Build

来源：`docs.x.ai/build/settings/reference`（下称 SR）、`docs.x.ai/build/overview`。

1. **项目级 `.grok/config.toml` 只贡献 `[mcp_servers]`、`[plugins]`、`[permission]`**，其余段（models、tools、sandbox 等）必须在用户级 `~/.grok/config.toml`。（SR「TOML Values」节）
   - 对 Reins：workspace 往项目级只写 MCP 的做法与官方约束一致，是安全边界。
2. **`[mcp_servers.<name>]` 完整参考**（SR）：stdio 用 `command`/`args`/`env`/`cwd`；HTTP 用 `url`/`headers`/`bearer_token_env_var`；公共项 `enabled`、`startup_timeout_sec`（默认 30）、`tool_timeout_sec`（默认 6000）、`tool_timeouts` map。字符串字段支持 `${VAR}` 展开，headers 还可用 `{{session_id}}`。
   - 对 Reins：印证 `aidocs/context/2026-09-15-mcp-config-mapping.md` 的变量语义记录；`tool_timeout_sec` 默认值与整数秒约束可补充进映射文档。
3. **跨工具扫描开关默认全开**（SR 环境变量节）：`GROK_CLAUDE_SKILLS_ENABLED`、`GROK_CURSOR_SKILLS_ENABLED`、`GROK_CLAUDE_MCPS_ENABLED` 等。
   - 对 Reins：Grok 会自动吃 Claude/Cursor 的 skills 和 MCP 配置，skills 同步后 Grok 可能「额外」看到同一份内容，排障时注意来源叠加。
4. 其他环境变量：`GROK_HOME`（默认 `~/.grok`）、`XAI_API_KEY`、`GROK_DEFAULT_MODEL`、`GROK_SANDBOX`、`GROK_SUBAGENTS` 等（SR）。
5. `grok models` 列可用模型；`-m` 选择模型、`--reasoning-effort` 传思考等级（CLI help）。

## Claude Code

来源：`code.claude.com/docs/en/settings`（下称 S）、`…/settings-reference`（下称 SR）、`…/env-vars`、`…/model-config`。

1. **设置文件层级与热重载**（S）：user `~/.claude/settings.json` → project `.claude/settings.json` → local `.claude/settings.local.json` → managed；多数键改动即时生效，无需重启。
2. **`/effort` 自 v2.1.251 起把等级写到 `modelSettings`（按模型键），不再写 `effortLevel`**；`effortLevel` 仅是「未单独保存模型的默认等级」；`maxEffortLevel` 可设上限（v2.1.267+）。（SR effortLevel/modelSettings 节）
   - 对 Reins：用户自己跑过 `/effort` 后等级存在 `modelSettings`，Reins 的 remove 只清 `effortLevel`、不动 `modelSettings` 是正确行为（那是用户自己的数据），移除后 Claude 侧仍可能有自己的等级覆盖。
3. **`env` 块语义**（SR env 节）：值覆盖 shell 同名变量；空字符串视为未设置；多文件取最高优先级。
4. **凭据通道**（env-vars）：`ANTHROPIC_API_KEY` → `X-Api-Key` 头；`ANTHROPIC_AUTH_TOKEN` → `Authorization: Bearer`；`apiKeyHelper` 命令式凭据；`CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST` 可让宿主平台接管 provider 路由（此时 settings 里的 env 被忽略）。
   - 对 Reins：聚合平台写入 `ANTHROPIC_API_KEY` 走 X-Api-Key，与 GLM 等 Anthropic 兼容网关的惯例一致。
5. **网关模型发现**（env-vars）：`CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1` 时 `/model` 选择器会从网关 `/v1/models` 拉取模型。
   - 对 Reins：聚合平台若要求 Claude 侧能看到全部模型，提示用户开这个变量比逐个写 `model` 更省事（候选优化，未做）。
6. MCP 相关设置键：`enableAllProjectMcpServers`、`enabledMcpjsonServers`、`managedMcpServers`（SR）——项目级 `.mcp.json` 的审批开关，排障项目 MCP 不生效时先看这些。

## Pi

来源：`pi.dev/docs/latest/models`（下称 M）、`…/settings`（下称 S）、`…/custom-provider`（下称 CP）。

1. **models.json 模型元数据远不止 name**（M/CP）：`input`（模态）、`inputLimits`（图片 resize、请求体上限）、`promptCache`（缓存声明）、`compat`（兼容开关，官方明确「只对实测过的差异开启」）；`modelOverrides` 可改内置模型元数据而不替换列表。
   - 对 Reins：providers 域现在只写 `id/name/reasoning/contextWindow/maxTokens`，后续可考虑透传 input/inputLimits。
2. **`apiKey` 支持内插**（M）：`$VAR`、`${NAME}`、`!command`（命令在请求时执行、不缓存）。
   - 对 Reins：目前明文写 key 是产品决策；若未来想支持 env 引用，Pi 侧官方本就支持。
3. **凭据优先级**（M）：`--api-key` > `auth.json`（/login）> `models.json apiKey` > provider 环境变量。
   - 对 Reins：用户在 Pi 里 `/login` 过的凭据会压过我们写入的 `apiKey`，排障「应用了但不生效」先查这个。
4. `settings.json` 还有 `modelThinkingLevels`（按 `provider/modelId` 的启动思考等级），优先于全局 `defaultThinkingLevel`（S）。
5. 项目目录被 trust 后，其 settings/extensions 会在 Pi 进程内执行（安全边界，M/CP）。
6. 会话文件格式有专门参考页 `/docs/latest/session-format`——Reins 读取 Pi 会话的契约来源（本次未展开）。

## OpenCode

来源：`opencode.ai/v2/docs/config/`（下称 C）、`…/docs/providers/`（P）、`…/docs/migrate-v1/`（M）。

1. **配置合并顺序**（C）：全局 `~/.config/opencode/opencode.json(c)` → 项目逐层目录（从最远到最近）→ `<project>/.opencode/opencode.json(c)`。
   - 对 Reins：Reins 只写全局文件；项目级是用户自留地，Reins 不碰。
2. **凭据**（P）：`/connect` 把 API key 存 `~/.local/share/opencode/auth.json`；自定义 provider 的 `settings.apiKey` 明文也是官方形态。
3. **v1 配置运行时兼容**（M）：V2 读同一路径，内存归一化 supported V1/native V2 字段、不重写源文件；官方明确「supported V1 行为失效按兼容 bug 处理」。这是 Reins 对 v1 条目只读展示、可删除的契约依据。
4. **Windows 不认 `XDG_CONFIG_HOME`**（本机 v2.0.16 实测，官方文档未写 Windows 差异）：读 `%USERPROFILE%\.config\opencode`。Reins 的路径解析目前优先 XDG，两处可能分叉——待定是否去掉 XDG 优先级。
5. **运行形态**：`opencode run`/TUI 依赖后台服务（`server-connection`），隔离 HOME 首次启动会初始化运行时（本次真机验证踩到）；排障时 `opencode debug config` 只列配置来源、不做运行时校验。

## Codex

来源：`learn.chatgpt.com/docs/config-file/config-reference`（下称 CR）、`llms.txt`。

1. **文档访问方式**：`llms.txt` 是全量索引；任意页 URL 加 `.md` 取 Markdown——后续核对方便。
2. **config.toml 其他键**（CR）：`review_model`、`openai_base_url`（内置 openai provider 的端点覆盖）、`notify`、`profiles.<name>`；MCP 侧 `mcp_servers.<id>.bearer_token_env_var`、`auth`（命令式 bearer token，不与 `env_key`/`experimental_bearer_token` 混用）。
3. **验证工具**（codex --help 实测）：`codex doctor`（config/auth/runtime 诊断）、`codex debug prompt-input`（渲染模型可见输入）、`codex debug models`。providers 域端到端验证脚本可固化这三个命令。

## 待定问题

- ~~OpenCode Windows 路径~~ 已解决：`opencode_config_path` 去掉 `XDG_CONFIG_HOME` 优先级，统一按 `HOME/.config/opencode` 解析（Windows 上 OpenCode v2.0.16 实测不认 XDG；Reins 与工具读取位置保持一致）。
