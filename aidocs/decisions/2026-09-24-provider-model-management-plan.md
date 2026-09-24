# 聚合平台模型管理模块（providers 域）实施计划

- 日期：2026-09-24
- 状态：方案已确认，未开始实现
- 产物：新增一级业务域 `providers`，与 `session`、`workspace` 并列

## 1. 模块定位

- 前端：`src/features/providers/`；Rust：`src-tauri/src/providers/`。
- 壳层新增第三个一级模式「模型配置」（`src/app/stores/app.ts` 的 `AppMode` 扩展，`AppModeRail.vue` 增加入口）。
- 持久化：独立文件 `<Reins 数据目录>/providers.yaml`（与现有 `config.yaml` 同目录、互不解析）。
- 独立 `ProviderConfigStore`，不依赖 `WorkspaceConfigStore`、`AgentTargetId`、workspace target、Skills、MCP 或会话数据。
- DTO 由 ts-rs 导出到 `src/features/providers/generated/`，沿用现有契约模式。

依赖红线：`providers` 域与 `sessions`、`workspace` 互不依赖；`shared`、`support` 保持现状约束。

## 2. 范围（已确认）

| 事项 | 结论 |
| --- | --- |
| 支持的工具应用 | Codex、Claude Code、OpenCode（仅 v2）、Pi、Grok Build |
| 不支持 | ZCode；任何项目级配置（不读写项目目录下的工具配置） |
| 官方 Provider | 仅 OpenAI、Anthropic；Reins 不管理（不存凭据/端点/模型），用户在各工具原生登录 |
| 聚合平台 | 除 OpenAI、Anthropic 外的一切平台/自建网关，由 Reins 管理端点、协议、模型、API Key |
| 部署记录 | 不持久化。工具配置文件是「已应用」状态的唯一事实来源，反显全部靠反读 |

## 3. 数据模型（providers.yaml）

```yaml
version: 1
providers:
  openrouter:
    label: OpenRouter
    protocol: openai_chat_completions   # openai_responses | openai_chat_completions | anthropic_messages
    base_url: https://openrouter.ai/api/v1
    models:
      - id: anthropic/claude-sonnet-4-5   # 发送给服务端的真实 model id
        label: Claude Sonnet 4.5
        # 以下全部可选；目标工具必填而未填时，该工具的「应用」按钮禁用
        context_window: 200000      # models.dev limit.context
        max_output_tokens: 64000    # models.dev limit.output
        supports_images: true       # models.dev modalities.input 含 image
        reasoning: true             # models.dev reasoning
        reasoning_levels: [low, medium, high]  # models.dev reasoning_options[].values 归一化
```

归一化思考等级集合：`none / minimal / low / medium / high / xhigh / max`；写入各工具时取交集。

## 4. 凭据（已确认的产品决策）

- API Key 存系统密钥管理：service 固定 `me.kazma.reins`，account = `providers.yaml` 中的平台 ID；平台重命名不改密钥账户名。
- 跨平台：macOS Keychain、Windows Credential Manager、Linux Secret Service（keyring 实现）；Linux 无 Secret Service 时明确报错，不做明文兜底。
- 用户点击「应用到工具」时，Reins 从系统密钥管理读取 Key，**明文写入目标工具的全局配置文件**。这是用户在知晓 AGENTS.md 安全约束后明确确认的边界；实现时密钥明文仍只允许出现在：Keychain、写入时的内存、目标工具配置文件。
- 禁止出现在：`providers.yaml`、Tauri 返回值、日志、错误、预览（预览中显示 `******`）、测试夹具。
- 首期仅支持 API Key 认证；不做 Bearer Token/OAuth/apiKeyHelper 类型区分。
- Claude Code 凭据固定写 `ANTHROPIC_API_KEY`。

## 5. 协议与工具能力

协议固定三种：`openai_responses`、`openai_chat_completions`、`anthropic_messages`。

| 工具 | 只操作的全局文件 | Provider 语义 | 接受协议 |
| --- | --- | --- | --- |
| Codex | `$CODEX_HOME/config.toml`（默认 `~/.codex/config.toml`） | 单活动 Provider，应用即替换 | 仅 `openai_responses` |
| Claude Code | `~/.claude/settings.json` | 单活动，替换 | 仅 `anthropic_messages` |
| OpenCode v2 | `$XDG_CONFIG_HOME/opencode/opencode.json` | 多 Provider 并存，新增 | `openai_chat_completions` 优先；`openai_responses` 待验证 |
| Pi | `$PI_CODING_AGENT_DIR/models.json` + `settings.json` | 多 Provider 并存，新增 | 三种均可 |
| Grok Build | `$GROK_HOME/config.toml`（默认 `~/.grok/config.toml`） | 多 Provider 并存，新增；项目 `.grok/` 不加载模型配置 | 三种均可 |

内部注册键统一 `reins-<平台ID>`，避免与工具内置 Provider 冲突；反显按该前缀识别 Reins 条目。

## 6. 各工具写入契约

| 工具 | 写入内容 |
| --- | --- |
| Codex | `model_provider="reins-x"`、`model=<id>`、`model_reasoning_effort`、`[model_providers.reins-x]`（name、base_url、wire_api="responses"、静态 Bearer Token 字段） |
| Claude Code | `env.ANTHROPIC_BASE_URL`（聚合平台必写；不写官方端点）、`env.ANTHROPIC_API_KEY`、`model`、`effortLevel`；保留文件中其他键 |
| OpenCode v2 | `provider["reins-x"]`（npm 包按协议选择、`options.baseURL`、`options.apiKey` 明文、`models`）+ 顶层 `model="reins-x/<id>"` |
| Pi | `models.json` 的 `providers["reins-x"]`（baseUrl、api、apiKey、models 含 reasoning 等元数据）+ `settings.json` 的 `defaultProvider`、`defaultModel`、`defaultThinkingLevel` |
| Grok Build | `[model_providers.reins-x]`（base_url、api_backend）+ 每个 `[model."reins-x--<id>"]`（model、name、model_provider、api_key；`anthropic_messages` 用 `extra_headers` 携带 `x-api-key` 与 `anthropic-version`）+ `[models] default`、`default_reasoning_effort` |

移除规则：

- 只删 `reins-` 前缀且内容仍可识别的条目；用户手工改过的漂移配置不自动删。
- Claude Code 仅当 `ANTHROPIC_BASE_URL` 与该平台一致才清（避免读密钥比对）。
- OpenCode/Pi/Grok 的默认模型引用待删条目时，先要求切换默认再移除。
- Codex/Claude 应用新平台即替换旧聚合平台配置。

## 7. 操作流程

- **应用**：能力校验（协议兼容 + 该工具必填高级参数齐全，缺失则按钮禁用并列出缺失项）→ 勾选模型 + 指定默认模型 + 可选默认思考等级（取模型 `reasoning_levels` ∩ 工具支持集）→ 读 Keychain → 预览（文件路径 + 变更 diff + 能力警告，密钥脱敏）→ 确认 → 原子写入目标文件。
- **反显**：`get_providers_state` 反读五个工具配置；条目状态 `已应用 / 漂移 / 外部配置 / 不支持`。外部配置（非 `reins-` 前缀、Claude 的非匹配 base_url）只展示不纳管。
- **删除平台**：删元数据 + Keychain 项；反读提示仍被引用的工具并要求用户确认，不自动清理外部配置。
- **模型目录**：平台拉取 + 手工新增并存。拉取：显式触发、固定超时（约 10s）、Key 只用于本次请求不落日志；返回列表由用户勾选后加入目录，失败可回退手工新增。
- **models.dev 元数据补全**：运行时按需拉取 `https://models.dev/api.json` 并缓存到 Reins 数据目录；平台/模型编辑页提供「从 models.dev 补全」。匹配规则：模型 id 去掉 `provider/` 前缀后优先在 models.dev 同名 provider 下精确匹配，否则全库按 id 搜索，多候选列出供用户选择；只预填空缺字段，不覆盖用户已填值。

## 8. 实现结构

```
src-tauri/src/providers/
  mod.rs          # 域入口、re-export
  config.rs       # ProviderConfigStore：providers.yaml 读写、进程内锁、原子写（复用 write_atomic 模式）
  types.rs        # ts-rs 导出 DTO
  commands.rs     # Tauri 命令
  keychain.rs     # keyring 封装：set/delete/exists/read；read 不暴露给前端
  catalog.rs      # 按协议拉取模型列表 + models.dev 缓存与匹配
  apps/
    mod.rs        # 适配器注册表 + 能力表（协议、必填字段、思考等级、替换/并存语义）
    codex.rs / claude.rs / opencode.rs / pi.rs / grokbuild.rs
```

命令：`get_providers_state`、Provider CRUD、`set_provider_key` / `remove_provider_key`、`fetch_provider_models`、`fetch_modelsdev_catalog`、`apply_provider_to_app`、`remove_provider_from_app`。

新增依赖：`keyring`（凭据）、`reqwest`（模型/目录拉取，blocking + rustls）。

前端：

- `AppMode` 增加 `"providers"`，rail 图标与文案「模型配置」。
- `src/features/providers/ProvidersWorkspace.vue`：两个页签——`平台`（聚合平台 CRUD、Key 管理、模型目录、拉取、models.dev 补全）、`工具应用`（五个应用卡片，反读状态、应用弹窗、移除）。
- `api.ts` 封装全部 invoke；类型只来自 `generated/`。

## 9. 实施顺序

1. 域骨架：`providers` 模块 + `AppMode` 第三项 + 空页面 + `pnpm codegen`。
2. Provider CRUD + Keychain 服务（set/delete/exists，read 仅内部使用）。
3. 模型目录：手工新增 + 协议拉取。
4. 五个工具的只读反读 inspection。
5. 逐个实现 apply/remove：Codex → OpenCode → Pi → Grok Build → Claude Code，每个都在隔离目录验证后进入下一个。
6. models.dev 补全与前端完整 UI。
7. 文档：更新 `2026-05-22-domain-boundaries.md` 增补 providers 域红线、README 功能说明。

## 10. 测试与验收

- 单测：providers.yaml 解析/序列化往返；各适配器「生成 → 解析 → 幂等」；协议/必填字段门控；默认模型引用移除拒绝；漂移识别。
- Keychain：trait 抽象 + 测试用假后端；真实后端只在手动验证时使用。
- 隔离环境端到端（虚拟 HOME / `CODEX_HOME` / `PI_CODING_AGENT_DIR` / `XDG_CONFIG_HOME` / `GROK_HOME` + 假密钥后端）：
  apply → 目标 CLI 解析成功 → 反读 = 已应用 → 再次 apply 无变更 → remove → 配置还原。
  - Codex：`codex --strict-config doctor`
  - OpenCode：`opencode debug config`
  - Pi：`pi --list-models`
  - Grok Build：`grok inspect --json`
  - Claude Code：`claude doctor` / `/status`
- 安全验收：全流程（含日志、错误、预览、命令输出）不泄露密钥明文，`providers.yaml` 无密钥痕迹。
- 工程门禁：`pnpm codegen`、`cargo fmt --check`、`cargo test`、`pnpm test`、`vue-tsc`、`pnpm build` 全绿。

## 11. 实现期待验证的 unverified 项

- Codex 静态 Bearer Token 字段（`experimental_bearer_token`）运行时接受性，及 `model_reasoning_effort` 合法取值。
- Claude Code `effortLevel` 的键名、取值与最低版本要求（本机未安装 Claude Code）。
- OpenCode v2 对 `openai_responses` 的 npm 适配与思考等级配置是否存在。
- Pi `models.json` 各协议的 `api` 取值与必填模型元数据。
- Grok `default_reasoning_effort` / `reasoning_efforts` 合法取值。
- 各聚合平台模型列表接口差异（`/v1/models` 形态、鉴权头）。
- models.dev `reasoning_options` 取值与各工具等级的映射表。

## 12. 背景与取舍记录

- `applications` 部署表方案已否决：不持久化应用关系，反显以工具配置文件为准。
- 「官方」口径按产品定义：仅 OpenAI、Anthropic；其余一律归聚合平台管理（即使底层是单一自有模型的服务商）。
- 凭据写入工具配置采用明文，是用户在知晓项目安全约束后对「系统密钥管理 + 应用时明文落盘」的明确确认；第一版不做凭据类型扩展。
- OpenCode 仅支持 v2；旧版本不兼容、不写入。
- 思考等级参考 models.dev 的 `reasoning` / `reasoning_options` 字段口径，归一化后存储，写入时按工具取交集。
