# providers 域工具契约二次核对（官方文档来源）

- 日期：2026-09-25
- 性质：契约核对记录。逐项比对 providers 域五个工具的写入/反读契约与官方文档，给出结论、来源与后续动作。
- 结论速览：写入契约主体与官方一致；发现 3 个不一致（Pi `off` 值、Claude 思考等级值域、OpenCode 思考等级声明未实现）与 2 条风险备注（Grok `model_providers` 未文档化、OpenCode Windows 路径 XDG 不生效）。

## 来源

| 工具 | 页面 | 说明 |
| --- | --- | --- |
| Codex | https://learn.chatgpt.com/docs/config-file/config-reference | config.toml 完整参考（`/docs/...` 加 `.md` 可取 Markdown） |
| Codex | https://learn.chatgpt.com/llms.txt | 文档索引 |
| Claude Code | https://code.claude.com/docs/en/settings-reference | `effortLevel`/`env`/`model` 键参考（加 `.md` 取 Markdown） |
| Claude Code | https://code.claude.com/docs/en/env-vars | `ANTHROPIC_BASE_URL`/`ANTHROPIC_API_KEY` 语义 |
| OpenCode | https://opencode.ai/v2/docs/config/ | v2 配置位置与顶层键 |
| OpenCode | https://opencode.ai/v2/docs/providers/ | v2 规范 provider schema（`providers`/`package`/`settings`） |
| OpenCode | https://opencode.ai/v2/docs/migrate-v1/ | v1 格式兼容承诺（内存归一化、不改源文件） |
| Pi | https://pi.dev/docs/latest/models | models.json 形态、`api` 取值示例 |
| Pi | https://pi.dev/docs/latest/settings | `defaultProvider`/`defaultModel`/`defaultThinkingLevel` 及枚举 |
| Pi | 本机 `@earendil-works/pi-ai` 包 `dist/types.d.ts` | `KnownApi` 枚举（openai-completions/openai-responses/anthropic-messages 等全部合法值） |
| Grok Build | https://docs.x.ai/build/overview | 自定义模型入口、`~/.grok/config.toml` 路径 |
| Grok Build | https://docs.x.ai/build/settings/reference | config.toml 全量键参考（`api_backend`、`api_key`、`extra_headers`、`[models]`） |

## 逐工具核对结论

### Codex（`$CODEX_HOME/config.toml`，协议仅 `openai_responses`）

| 契约项 | 结论 | 依据 |
| --- | --- | --- |
| `wire_api` 仅 `"responses"` | ✅ 一致 | 官方原文 "responses is the only supported value, and it is the default when omitted" |
| `[model_providers.<id>]` 的 `name`/`base_url` | ✅ 一致 | config-reference |
| `experimental_bearer_token` 静态 Bearer | ✅ 字段存在（官方标注 discouraged，建议 `env_key`） | config-reference |
| `model_provider`/`model` | ✅ 一致 | config-reference |
| `model_reasoning_effort` 值域 | ⚠️ 部分确认 | 官方列举 `low/medium/high/xhigh/max/ultra`（"such as" 非穷举）；我们写出的 `none`/`minimal` 未在列，待真机验证 |

### Claude Code（`~/.claude/settings.json`，协议仅 `anthropic_messages`）

| 契约项 | 结论 | 依据 |
| --- | --- | --- |
| `env.ANTHROPIC_BASE_URL`（网关/代理端点） | ✅ 一致 | settings-reference `env` 示例 + env-vars 表 |
| `env.ANTHROPIC_API_KEY`（以 `X-Api-Key` 发送） | ✅ 一致 | env-vars 表 |
| `model` 键 | ✅ 一致 | settings-reference |
| `effortLevel` 键名 | ✅ 一致 | settings-reference |
| `effortLevel` 值域 `low/medium/high/xhigh/max` | ❌ 不一致 | 官方值域不含 `off`/`minimal`；我们能力表声明 7 档并按小写写出 `none`/`minimal`，选这两档会写出无效值 |

### OpenCode v2（`~/.config/opencode/opencode.json`，协议 `openai_chat_completions` + `anthropic_messages`）

| 契约项 | 结论 | 依据 |
| --- | --- | --- |
| v2 规范 schema：`providers` + `package` + `settings.baseURL`/`settings.apiKey` + `models` | ✅ 一致 | v2 providers 文档 |
| `package` 按协议：`@opencode/ai/providers/openai-compatible` / `@opencode/ai/providers/anthropic` | ✅ 一致 | v2 providers 文档原生包清单 + 端点代理示例（anthropic + settings.baseURL） |
| 顶层 `model = "<provider>/<model>"` | ✅ 一致 | v2 config 文档 |
| 全局配置路径 `~/.config/opencode/opencode.json` | ✅ 一致 | v2 config 文档 |
| v1 遗留 schema（`provider`/`npm`/`options`） | ✅ 只读展示、可删除，不写入 | migrate-v1：v2 运行时继续兼容 v1（内存归一化、不改源文件） |
| `openai_responses` 不放行 | ❌→✅ 已恢复放行（2026-09-27 复测） | v2.0.18 复测 `openai/responses` 端到端可用；`openai-compatible/responses` 仍异常，见 2026-09-27 补充节 |
| 思考等级 | ❌ 不一致 | 能力表声明 7 档全集、弹窗可选，但 `apply` 不消费 `default_reasoning_level`；v2 文档亦未确认思考等级写法 |
| Windows 路径 | ✅ 已对齐 | 本机实测 OpenCode v2.0.15+ 不认 `XDG_CONFIG_HOME`（读 `%USERPROFILE%\.config\opencode`）；Reins 已去掉 XDG 优先级，统一按 `HOME/.config` 解析 |

### Pi（`$PI_CODING_AGENT_DIR/models.json` + `settings.json`，三协议全支持）

| 契约项 | 结论 | 依据 |
| --- | --- | --- |
| `api` 三值 `openai-completions`/`openai-responses`/`anthropic-messages` | ✅ 全部合法 | `KnownApi` 枚举（本机 pi-ai 包 types.d.ts）+ models 页示例 |
| `providers.<id>` 的 `baseUrl`/`apiKey`/`models` | ✅ 一致 | models 页示例 |
| `defaultProvider`/`defaultModel`/`defaultThinkingLevel` | ✅ 一致 | settings 页 |
| 思考等级枚举 `off\|minimal\|low\|medium\|high\|xhigh\|max` | ❌ 不一致（1 档） | 官方枚举首档是 `"off"`；我们 `ReasoningLevel::Off.as_str()` 写出 `"none"`，选「无」时 Pi 拿到无效值；其余 6 档合法 |

### Grok Build（`$GROK_HOME/config.toml`，三协议全支持）

| 契约项 | 结论 | 依据 |
| --- | --- | --- |
| `api_backend: chat_completions \| responses \| messages` | ✅ 一致 | settings-reference 原文，与实测值相同 |
| `[model.<id>]` 的 `api_key`/`base_url`/`name`/`extra_headers` | ✅ 一致 | settings-reference（`extra_headers` 可承载 anthropic 头） |
| `[model.<id>].context_window` | ✅ 已补写（2026-09-27） | settings-reference 原文「Context window size (drives auto-compact timing)」；此前未写，grok 缺该字段按 200000 兜底，见 2026-09-27 补充节 |
| `[model.<id>].supports_reasoning_effort` | ✅ 已补写（2026-09-27） | settings-reference 原文「Reasoning controls when supported」；此前未写，grok 判该模型不支持思考等级并丢弃 `[models].default_reasoning_effort`，见 2026-09-27 补充节 |
| `[model.<id>]` 的图像/多模态字段 | ⛔ 不存在 | settings-reference 全表无 image/vision 字段；1.0.41 实测 `supports_images`/`input`/`supports_vision` 被静默忽略，模型元数据无变化。图片相关只有 `[models].image_description`（语义为图片描述模型），Reins 不写图像输入 |
| `[models].default` / `[models].default_reasoning_effort` | ✅ 键名与位置一致 | settings-reference（位于 `[models]` 段） |
| `[model_providers]` 表 + `model_provider` 字段 | ⚠️ 备注：官方未文档化 | 官方简化格式是 `[model.<id>]` 直接带 `base_url`/`env_key`；我们的写法来自 grok 1.0.40 实测（`grok inspect --json` 可识别），需真机复测当前版本 |
| `default_reasoning_effort` 值域 | ✅ 已实测 | 2026-09-27 grok 1.0.41：接受 `none/minimal/low/medium/high/xhigh/max`，拒绝 `off`（报 `invalid reasoning effort`）。Grok 交互 UI 只渲染 low/medium/high/xhigh 四档并把用户选择写回 config；Reins 已把 `Max` 档映射写出 `"xhigh"`（请求层两者等价，见下条），其余档同名直写 |
| `messages` 后端 base_url 请求路径 = `{base_url}/messages`，不补 `/v1` | ❌→✅ 已修复 | 2026-09-27 grok 1.0.41 本地监听实测（配置 `/v1` 结尾 → 请求 `/v1/messages` 正常完成；不带 → 请求 `/messages`）；与 Anthropic SDK「base_url 不带版本段」约定不同，grokbuild 写入时对 `anthropic_messages` 规范化补 `/v1`，反读按同一口径比对 |

## 真机验证结果（2026-09-25）

本机版本：codex-cli 0.145.0、grok 1.0.40、opencode v2.0.16、pi 0.87.1。隔离目录验证，端点用不可达地址区分「配置错」与「网络错」。

除配置解析外，四个 CLI 均做了「注册级」验证：工具自身的模型列表/请求路径确认 Reins 写入的条目被识别并使用。

| 工具 | 验证方式 | 结果 |
| --- | --- | --- |
| Codex | 隔离 `CODEX_HOME` + apply 形态配置；`codex doctor`、`codex debug prompt-input`、`codex exec`（正负对照） | ✅ config loaded；`codex exec` 请求打向 `http://127.0.0.1:9/v1/responses`（正是我们配置的 provider + responses 协议）；负对照 `nonexistent-provider` 立即报 `Model provider not found` |
| Grok | 隔离 `GROK_HOME` + apply 形态配置；`grok inspect --json`、`grok models`（正负对照） | ✅ `grok models` 列出两个 reins 模型、默认模型标记正确、`api_key` 被识别；对照无配置环境无 reins 条目、坏模型名立即报 `unknown model id` |
| Pi | 隔离 `PI_CODING_AGENT_DIR` + apply 形态的 models.json/settings.json；`pi -p`（正负对照） | ✅ 默认模型解析并发起请求（Connection error 为端点不可达）；负对照 `No models match pattern` 明确警告 |
| OpenCode | 全局 v2 条目经用户实际应用验证：`opencode models` | ✅ `reins-glm/glm-5.3` 等三个已应用模型出现在工具模型列表 |
| Claude Code | 本机未安装 | 未验证（unverified） |

补充验证记录（同日）：

- Codex `model_reasoning_effort` 取 `none/minimal/low` 时 `codex doctor` 均 `config loaded`，`debug prompt-input` 正常渲染；请求时行为需真实端点。
- OpenCode `openai_responses`：对照实验（同一探针条目换包名）显示 anthropic 包注册成功、`openai/responses` 与 `openai-compatible/responses` 在 v2.0.16 均静默加载失败，维持不放行；后续版本需复测。

## 2026-09-27 补充：Grok messages 后端 base_url 实测（grok 1.0.41）

方法：隔离 `GROK_HOME` + 本地 HTTP 监听器，配置 `api_backend = "messages"` + `extra_headers`（假 key），`grok -p` 单轮请求，观察监听器收到的请求。

- base_url 以 `/v1` 结尾 → 请求 `POST /v1/messages`，携带 `x-api-key` 与 `anthropic-version`，grok 正常完成。
- base_url 不带 `/v1` → 请求 `POST /messages`（无版本段），打真实端点（如智谱 `…/api/anthropic`）会 404。此为 2026-09-26 用户「glm 应用到 Grok 不可用」的根因；`x-api-key` 头本身传递正常，智谱官方也认该头（官方 curl 示例）。
- 附带观察：grok 会话开始会向自定义 provider 端点发一个标题生成请求，模型名为 `grok-4.6`（非配置模型），打到第三方端点会报模型不存在，但不阻塞主对话。
- `default_reasoning_effort` 值域实测：`off` 报 `invalid reasoning effort: "off" (expected one of: none, minimal, low, medium, high, xhigh, max)`，其余 7 值全部被 `grok models` 接受；缺省不写也接受。Grok 交互 UI 自行渲染四档（xhigh/high/medium/low）并把用户选择写回 config，与配置层 7 值枚举、Reins 弹窗档位均不同源。
- `default_reasoning_effort` 请求层映射实测（messages 后端，本地监听）：`low/medium/high/xhigh/max` 五档发出的 `thinking` 参数完全相同（`{"type":"adaptive","display":"summarized"}`），无量化差异；`none`/`minimal`/缺省则不带 `thinking` 字段。即对第三方 messages 端点，high 以上各档在请求上等价，`max` 与 `xhigh` 无请求层区别，且 Grok UI 无 max 档可选。
- 另注意到 `chat_completions`/`responses` 后端的路径拼接规则未实测；OpenAI 生态习惯 base_url 带 `/v1`，暂按用户配置原样写入。

## 2026-09-27 补充：Grok per-model 元数据字段实测（grok 1.0.41）

方法：隔离 `GROK_HOME` + 合成 config.toml（两个自定义模型对照：裸模型 / 带 `context_window` 等字段），`grok -p --debug-file` 与 `grok models`，读 debug 日志与 `session/new` 响应的 `availableModels[]._meta`；另用三个候选字段名探测未知字段行为。

- **`context_window` 缺失时按 200000 兜底**：debug 日志原文 `new model missing context_window, defaulting to 200000 -- set context_window in [model.<key>] to override`；模型 _meta 为 `totalContextTokens: 200000`。写上 `context_window = 128000` 后日志不再出现该条、_meta 为 `128000`。该值驱动自动压缩时机（`[session].auto_compact_threshold_percent` 默认 85）——Reins 此前不写该字段，对非 200K 窗口的模型时机失准。处置：apply 现在有元数据就写 `context_window`，新增测试 `grok_apply_writes_context_window_and_reasoning_support`、`grok_apply_omits_absent_model_metadata`。
- **`supports_reasoning_effort` 是默认档生效前提**：裸模型 + `[models].default_reasoning_effort = "high"` 时 grok 打 WARN `reasoning_effort: model does not support effort; ignoring it session_id=… model=test-bare effort=high`；模型写 `supports_reasoning_effort = true` 后 _meta 出现 `supportsReasoningEffort: true`（`reasoning_effort = "low"` 亦反映到 _meta 的 `reasoningEffort`）。即 Reins 此前写的 `default_reasoning_effort` 在其自定义模型上被静默忽略。处置：`model.reasoning == Some(true)` 时写 `supports_reasoning_effort = true`。
- **图像输入无配置落点**：`[model.<id>]` 表无任何 image/vision 字段（settings-reference 全表核对），探测 `supports_images = true`、`input = ["text","image"]`、`supports_vision = true` 三个候选字段全部被静默忽略（debug 日志无 unknown-field 警告，模型 _meta 与裸模型完全相同）；连内置 `grok-4.6` 的 _meta 也没有图片能力字段。Reins 的 `supports_images` 元数据在 Grok 侧不写，由应用弹窗明示。
- 交叉观察：`_meta.agentType` 均为 `grok-build-plan`；`max_completion_tokens` 实测被接受（解析无 unknown-field 警告），但它是采样上限而非元数据落点，Reins 不写。
- 上述事实与各工具写入口径汇总到能力表 `unwritten_model_fields`，经 `ProviderAppState` 下发到应用弹窗；对应测试 `app_states_declare_unwritten_model_fields`。

## 2026-09-27 补充：OpenCode openai_responses 复测（opencode v2.0.18）

方法：隔离目录 + 项目级 `opencode.json`，同一探针条目仅换 `package`；注册级用 `opencode models`（负对照：无配置目录不出现探针条目，确认按目录解析无缓存串扰）；请求级用 `opencode run` + 本地 HTTP 监听器（baseURL `http://127.0.0.1:18081/v1`）。

- `@opencode/ai/providers/openai/responses`：注册成功；请求 `POST {baseURL}/v1/responses`、`Authorization: Bearer`、标准 Responses wire format（`model` + `input` + `instructions`），监听器返回的最小 response 被正常消费、会话继续下一步。✅ 端到端可用。
- `@opencode/ai/providers/openai-compatible/responses`：`opencode models` 可注册，但 `opencode run` 初始化即报 `Cannot find package '@opencode/ai'`，两次复现。❌ 不采用。
- 对照组 `@opencode/ai/providers/openai-compatible`（chat）：注册、请求均正常。

处置：OpenCode 能力表恢复放行 `openai_responses`，apply 写 `openai/responses` 包，新增测试 `opencode_apply_responses_uses_openai_responses_package`；compatible/responses 的包解析缺陷属 OpenCode 侧问题，上游修复后再评估切换。

## 处置结果

1. ✅ Pi `off` 档：单独映射为 `"off"`（pi.rs `thinking_level`），新增测试 `pi_off_level_writes_off`。
2. ✅ Claude 能力表收窄为官方值域 low/medium/high/xhigh/max，`off/minimal` 不再放行。
3. ✅ OpenCode 能力表思考等级清空（apply 本就不消费、文档未确认写法），弹窗不再出现该选项。
4. ✅ Codex 能力表收掉 off 档（写入 "none" 无文档依据；minimal 保留，用户决策 A），新增测试 `codex_off_reasoning_level_is_rejected`。
5. ✅ Grok `Max` 档映射写出 `"xhigh"`（2026-09-27：Grok UI 无 max 档、messages 后端请求层 max 与 xhigh 等价），新增测试 `grok_max_reasoning_writes_xhigh`；`messages` 后端 base_url 写入补 `/v1`，新增测试 `grok_anthropic_base_url_gets_v1_suffix_and_inspect_stays_applied`。
6. ✅ OpenCode `openai_responses` 恢复放行（2026-09-27 v2.0.18 复测 `openai/responses` 端到端可用；`openai-compatible/responses` 仍有包解析缺陷，不采用），apply 写 `openai/responses`，新增测试 `opencode_apply_responses_uses_openai_responses_package`。
7. ✅ Grok 逐模型补写 `context_window`（有元数据时）与 `supports_reasoning_effort = true`（`reasoning` 为真时），新增测试 `grok_apply_writes_context_window_and_reasoning_support`、`grok_apply_omits_absent_model_metadata`（2026-09-27，依据见上节）。
8. ✅ 能力表新增 `unwritten_model_fields`，`ProviderAppState` 下发到应用弹窗明示不写入的模型元数据（Codex/Claude/OpenCode 四类全列、Grok 列最大输出与图像输入、Pi 为空），新增测试 `app_states_declare_unwritten_model_fields` 与前端 `model.test.ts` 用例。

## 后续动作

1. ~~真机验证~~ 已完成，结果见上节。
2. ~~修复 ❌ 三项~~ 已完成，见处置结果。
3. plan 文档 unverified 项已同步更新（2026-09-25）。
4. ~~待定：OpenCode 在 Windows 上不认 `XDG_CONFIG_HOME`~~ 已解决：路径解析去掉 XDG 优先级，统一按 `HOME/.config/opencode` 解析（2026-09-25）。
