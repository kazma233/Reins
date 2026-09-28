# providers 域工具契约二次核对（官方文档来源）

- 日期：2026-09-25
- 性质：契约核对记录。逐项比对 providers 域五个工具的写入/反读契约与官方文档，给出结论、来源与后续动作。
- 结论速览：写入契约主体与官方一致；发现 3 个不一致（Pi `off` 值、Claude 思考等级值域、OpenCode 思考等级声明未实现）与 2 条风险备注（Grok `model_providers` 未文档化、OpenCode Windows 路径 XDG 不生效）。
- 后续复测：2026-09-27 起按日期追加补充节（Grok base_url 与逐模型元数据、OpenCode `openai_responses` 复测、其余四个工具的模型元数据配置面），结论以对应日期的小节为准。

## 来源

| 工具 | 页面 | 说明 |
| --- | --- | --- |
| Codex | https://learn.chatgpt.com/docs/config-file/config-reference | config.toml 完整参考（`/docs/...` 加 `.md` 可取 Markdown）；2026-09-28 起该域名对本机返回 Forbidden，改按源码核对 |
| Codex | https://learn.chatgpt.com/llms.txt | 文档索引 |
| Codex | `codex-rs/core/config.schema.json`（`rust-v0.145.0` 标签） | 由 `ConfigToml` 生成的配置键权威清单（`model_context_window`、`model_auto_compact_token_limit` 等） |
| Codex | `codex-rs/models-manager/src/model_info.rs`（同标签） | `model_context_window` 的覆盖逻辑与未知模型的兜底元数据 |
| Claude Code | https://code.claude.com/docs/en/settings-reference | `effortLevel`/`env`/`model` 键参考（加 `.md` 取 Markdown） |
| Claude Code | https://code.claude.com/docs/en/env-vars | `ANTHROPIC_BASE_URL`/`ANTHROPIC_API_KEY` 语义；`CLAUDE_CODE_MAX_CONTEXT_TOKENS`/`CLAUDE_CODE_MAX_OUTPUT_TOKENS`/`CLAUDE_CODE_AUTO_COMPACT_WINDOW` 等模型窗口与输出旋钮 |
| OpenCode | https://opencode.ai/v2/docs/config/ | v2 配置位置与顶层键 |
| OpenCode | https://opencode.ai/v2/docs/providers/ | v2 规范 provider schema（`providers`/`package`/`settings`）；模型字段表（`capabilities`/`limit`/`variants`） |
| OpenCode | https://opencode.ai/v2/docs/models/ | 模型字段示例与目录外模型的兜底假设（200000 上下文 / 32000 输出 / text+image 输入） |
| OpenCode | https://opencode.ai/config.json | `$schema` 指向的 JSON Schema；provider 外壳仍是 v1 形态，模型条目字段（含 `limit`/`modalities`）可作对照 |
| OpenCode | https://opencode.ai/v2/docs/migrate-v1/ | v1 格式兼容承诺（内存归一化、不改源文件） |
| Pi | https://pi.dev/docs/latest/models | models.json 形态、`api` 取值示例、`input`/`inputLimits` 字段 |
| Pi | https://pi.dev/docs/latest/settings | `defaultProvider`/`defaultModel`/`defaultThinkingLevel` 及枚举 |
| Pi | 本机 `@earendil-works/pi-ai` 包 `dist/types.d.ts` | `KnownApi` 枚举（全部合法值）与模型字段 `contextWindow`/`maxTokens`/`reasoning` |
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

## 2026-09-28 补充：其余四个工具的模型元数据配置面核对

起因：Grok 补写 `context_window` / `supports_reasoning_effort`（见上节）后，复查 Codex、Claude Code、OpenCode、Pi 是否同样存在「官方有配置面、Reins 未使用」的模型元数据缺口。

方法：以各工具生成的配置 schema、源码与官方文档为准；本机版本 `codex-cli 0.145.0`、`claude 2.1.185`、`opencode v2.0.18`、`pi 0.87.1`。OpenCode 与 Codex 另做隔离实测（临时项目目录 + 合成配置，`opencode models` / `opencode debug config`）。本次只读核对，未改代码。

| 工具 | 上下文窗口 | 最大输出 | 图像输入 | 思考能力 |
| --- | --- | --- | --- | --- |
| Codex | `model_context_window` 可用，Reins 未写 | 无字段 | 无字段 | `model_reasoning_effort` 已写 |
| Claude Code | `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS` 可用，未写 | `env.CLAUDE_CODE_MAX_OUTPUT_TOKENS` 可用，未写 | 无字段 | `effortLevel` 已写 |
| OpenCode v2 | `models.<id>.limit.context` 可用，未写 | `models.<id>.limit.output` 可用，未写 | `models.<id>.capabilities.input` 可用，未写 | v2 无对应字段（维持不写） |
| Pi | `contextWindow` 已写 | `maxTokens` 已写 | `input`（text/image）已写 | `reasoning` 已写 |

- **Codex**：`codex-rs/core/config.schema.json`（由 `ConfigToml` 生成，`rust-v0.145.0` 标签，与本机版本对应）含顶层键 `model_context_window`（"Size of the context window for the model, in tokens."）与 `model_auto_compact_token_limit`；`models-manager/src/model_info.rs` 中 `config.model_context_window` 覆盖模型窗口并按 `max_context_window` 收敛。不在内置目录中的模型走兜底描述符并告警 `Unknown model {slug} is used. This will use fallback model metadata.`，兜底 `context_window: 272000`、`input_modalities` 取默认值、`supported_reasoning_levels` 为空集。schema 中无模型输出上限键（`tool_output_token_limit` 是工具输出截断，语义不同）。
- **Claude Code**：env-vars 文档两条正对网关场景——`CLAUDE_CODE_MAX_CONTEXT_TOKENS`（原文：经 `ANTHROPIC_BASE_URL` 路由到「窗口与内置不符」的模型时用它纠正）、`CLAUDE_CODE_MAX_OUTPUT_TOKENS`（原文：对不认识的网关模型 ID 默认 32000）。两者都能写进我们已在用的 `settings.json` → `env`（该块覆盖同名 shell 变量）。相关旋钮另有 `CLAUDE_CODE_AUTO_COMPACT_WINDOW`（100000–1000000）、`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`、`CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT`（文档标注需 2.1.223+，本机 2.1.185）。无图像能力字段。
- **OpenCode v2**：v2 providers 文档的模型字段表含 `limit`（Context/input/output token limits）与 `capabilities`（tools + 接受的输入/输出媒体类型）；models 页明确目录外模型按 200000 上下文、32000 输出、text+image 输入兜底，并注明「These are fallback assumptions, not detected capabilities. Set accurate capabilities and limit values when known」。隔离实测（v2.0.18，项目级 `opencode.json`）：写入 `limit{context:128000,output:16000}` 与 `capabilities{tools,input:[text,image],output:[text]}` 后 `opencode models` 注册成功、`opencode debug config` 原样解析出这些字段；同批写入的 v1 风格 `reasoning: true` 与 `temperature` 未出现在解析结果中（进一步印证 v2 不消费这两个键，与能力表不收思考等级的现状一致）。
- **Pi**：本地 `@earendil-works/pi-ai/dist/types.d.ts` 的 `contextWindow`/`maxTokens`/`reasoning` 与 apply 写入一致，`input` 模态见 models 文档示例；四类元数据无缺口。更细的 `inputLimits`/`promptCache`/`compat.*` 不属本次四个维度，按模型的 `modelThinkingLevels` 也不写（用全局 `defaultThinkingLevel`）。

影响与取舍（已采纳，见下节）：

- 三个工具对我们接入的聚合模型都按固定兜底值处理窗口/输出（Codex 272000、OpenCode 200000 与 32000、Claude 输出 32000），与模型真实值不符时自动压缩时机与输出上限失准，性质同 Grok 的 200000 兜底。
- Codex 与 Claude Code 的窗口/输出是「活动模型」级全局配置，不是 per-model：Reins 在这两处都是单活动 provider 且 `model` 写死为所选默认模型，因此写默认模型的窗口是自洽的；用户在工具内切换到其他模型时会失配，若采纳需先定策略（只写默认模型、限制单模型场景，或用 Claude 的 `modelSettings` 分担）。
- OpenCode 的 `capabilities.input` 正好对上已有元数据 `supports_images`。

未验证：

- OpenCode `limit`/`capabilities` 只验证到被运行时解析进配置（注册 + `debug config`），是否真正驱动压缩与输出上限未做请求级验证。
- Claude 两个 env 变量本次仅依据官方文档，未做真实网关端到端验证。
- Codex 兜底描述符 `supported_reasoning_levels` 为空集时，已写入的 `model_reasoning_effort` 在请求层是否被采纳未验证（配置加载层此前已验证可加载）。

## 2026-09-28 补充（二）：三处落点与 OpenCode 思考等级的落地

用户判定上一节的「有落点未写」是缺陷并要求修复，OpenCode 另指出思考等级可配。核对与实测结论：

- OpenCode 思考等级**确实可配**，但写法按协议分叉（上一轮只看到 `compatibility.reasoningField`，漏了模型级 `settings`）：
  - `providers.<id>.models.<modelId>.settings.reasoningEffort`（官方 models 页字段表：「settings — Provider-package options such as baseURL or reasoningEffort」）。实测（v2.0.18 + 本地监听器）：写入 `"reasoningEffort": "xhigh"` 后，`opencode run` 发出的 `POST /v1/chat/completions` 请求体带 `reasoning_effort: "xhigh"`。✅ 上线。
  - anthropic 包不消费 `reasoningEffort`（同法实测：请求体只有 model/system/messages/stream/max_tokens）；该包的思考入口是 `settings.thinking`：`{"type":"enabled","budgetTokens":16000}` → 请求体带 `thinking: {type:"enabled", budget_tokens:16000}`；`{"type":"disabled"}` → 带 `thinking: {type:"disabled"}`；**只写 `{"type":"enabled"}` 不带预算时请求不发出**（实测 captured=0），即 enabled 必须带预算。
- 处置（已实现，测试见处置结果第 9–12 条）：
  - Codex：写默认模型的 `model_context_window`；元数据缺失时仅在替换 Reins 自身配置的路径上清除，移除时按「值仍等于该模型元数据」归属清理。
  - Claude Code：写 `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS` 与 `env.CLAUDE_CODE_MAX_OUTPUT_TOKENS`（十进制字符串，官方同类变量示例为纯整数）；移除时随 `env` 块一并清掉。
  - OpenCode：逐模型写 `limit{context,output}`（两项齐全才写，官方 schema 里二者成对必填）、`capabilities{tools,input,output}`（`supports_images` 决定输入模态）、`settings` 思考设置。
  - OpenCode 能力表放行思考等级 none/minimal/low/medium/high/xhigh（`max` 不放行：openai 系无该档）；anthropic 包的档位→预算取固定阶梯 minimal 1024 / low 2048 / medium 8192 / high 16384 / xhigh 与 max 32768，`off` 写 `disabled`。该阶梯官方与 OpenCode 都未定义，属 Reins 的产品取值，换档位即换思考预算；**待定**：这组数值待确认（见后续动作第 6 条）。
  - 能力表 `unwritten_model_fields` 同步收窄：Codex 变「最大输出、图像输入、推理能力」，Claude 变「图像输入、推理能力」，OpenCode 与 Pi 为空，Grok 不变。

证据层级：OpenCode 为请求级实测（用 Reins 实际产出的 `opencode.json`，仅替换 baseURL 指向本地监听器）；Codex 为源码/schema 级（本机 `doctor` 不做未知键校验，未取得运行时证据）；Claude 为官方文档级（未做真实网关验证）。

## 处置结果

1. ✅ Pi `off` 档：单独映射为 `"off"`（pi.rs `thinking_level`），新增测试 `pi_off_level_writes_off`。
2. ✅ Claude 能力表收窄为官方值域 low/medium/high/xhigh/max，`off/minimal` 不再放行。
3. ✅ OpenCode 能力表思考等级清空（apply 本就不消费、文档未确认写法），弹窗不再出现该选项。
4. ✅ Codex 能力表收掉 off 档（写入 "none" 无文档依据；minimal 保留，用户决策 A），新增测试 `codex_off_reasoning_level_is_rejected`。
5. ✅ Grok `Max` 档映射写出 `"xhigh"`（2026-09-27：Grok UI 无 max 档、messages 后端请求层 max 与 xhigh 等价），新增测试 `grok_max_reasoning_writes_xhigh`；`messages` 后端 base_url 写入补 `/v1`，新增测试 `grok_anthropic_base_url_gets_v1_suffix_and_inspect_stays_applied`。
6. ✅ OpenCode `openai_responses` 恢复放行（2026-09-27 v2.0.18 复测 `openai/responses` 端到端可用；`openai-compatible/responses` 仍有包解析缺陷，不采用），apply 写 `openai/responses`，新增测试 `opencode_apply_responses_uses_openai_responses_package`。
7. ✅ Grok 逐模型补写 `context_window`（有元数据时）与 `supports_reasoning_effort = true`（`reasoning` 为真时），新增测试 `grok_apply_writes_context_window_and_reasoning_support`、`grok_apply_omits_absent_model_metadata`（2026-09-27，依据见上节）。
8. ✅ 能力表新增 `unwritten_model_fields`，`ProviderAppState` 下发到应用弹窗明示不写入的模型元数据（Codex/Claude/OpenCode 四类全列、Grok 列最大输出与图像输入、Pi 为空），新增测试 `app_states_declare_unwritten_model_fields` 与前端 `model.test.ts` 用例。
9. ✅ Codex 写 `model_context_window`（默认模型元数据；替换时无元数据则清、移除时按元数据归属清），新增测试 `codex_apply_writes_and_clears_model_context_window`、`codex_remove_clears_model_context_window`。
10. ✅ Claude Code 写 `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS` / `env.CLAUDE_CODE_MAX_OUTPUT_TOKENS`，移除时随 `env` 清掉，新增测试 `claude_apply_writes_context_and_output_env_and_remove_clears_them`。
11. ✅ OpenCode 逐模型写 `limit{context,output}` 与 `capabilities{tools,input,output}`（limit 两项齐全才写、模态跟随 `supports_images`），新增测试 `opencode_apply_writes_model_limit_and_capabilities`、`opencode_apply_omits_incomplete_limit_and_narrows_modalities`。
12. ✅ OpenCode 思考等级落地：能力表放行 none/minimal/low/medium/high/xhigh，openai 系写 `settings.reasoningEffort`、anthropic 包写 `settings.thinking`（档位→预算固定阶梯，`off` 写 disabled），新增测试 `opencode_apply_writes_reasoning_effort_or_thinking_budget`；能力表清单同步收窄（依据见 2026-09-28 补充（二））。

## 后续动作

1. ~~真机验证~~ 已完成，结果见上节。
2. ~~修复 ❌ 三项~~ 已完成，见处置结果。
3. plan 文档 unverified 项已同步更新（2026-09-25）。
4. ~~待定：OpenCode 在 Windows 上不认 `XDG_CONFIG_HOME`~~ 已解决：路径解析去掉 XDG 优先级，统一按 `HOME/.config/opencode` 解析（2026-09-25）。
5. ~~待决定：是否补写 Codex / Claude Code / OpenCode 的模型窗口与输出落点~~ 已采纳并实现（2026-09-28），见 2026-09-28 补充（二）与处置结果第 9–12 条；plan 文档 §6 写入口径已同步。
6. 待定（产品取值）：OpenCode anthropic 包的档位→思考预算阶梯（minimal 1024 / low 2048 / medium 8192 / high 16384 / xhigh 32768，`off` 写 disabled）。官方与 OpenCode 都没有该映射，现值是 Reins 自定；可选改为其它阶梯、所有非 off 档统一预算（档位仅表达开关），或对 anthropic 包不写 thinking（只在 openai 系提供商提供档位）。改动范围仅 `opencode.rs` 的 `thinking_budget_tokens` 与对应测试。
