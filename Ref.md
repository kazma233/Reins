# AI 编码工具上游源码索引（有源码 / 无源码）

最后更新:2026-10-13,基于六个仓库的一手源码调研。聚焦 reins 管理这些工具所需的
权威知识:会话转录格式、配置与凭据落点、provider/模型 schema、目录重定向与
CLI 边界。**上游会漂移,引用前重新核对**;各节的「调研基线」记录了本次调研用的
commit 与当时 latest tag。

- [openai/codex](https://github.com/openai/codex)（Apache-2.0）
- [earendil-works/pi](https://github.com/earendil-works/pi)（MIT）
- [xai-org/grok-build](https://github.com/xai-org/grok-build)（Apache-2.0）
- [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)（MIT）
- [anomalyco/opencode](https://github.com/anomalyco/opencode)（MIT）
- [zai-org/ZCode](https://github.com/zai-org/ZCode)（Apache-2.0）
- Claude Code：**无官方开源仓库**（见文末）

> 许可证纪律:一律只借知识(格式、schema、字段口径、行为语义),不复制代码。
> 六个仓库都是宽松许可(Apache-2.0/MIT),没有 GPL 传染问题,但实现一律按
> reins 自身技术栈重写;引用时带路径,结论沉淀到 `aidocs/`,不要把上游源码
> 片段搬进 `src/`。
>
> 证据纪律:契约优先级 = **已安装运行时的实际行为 > 对应发布 tag 的源码/文档 >
> 默认分支源码 > 打包产物的反编译补充**。三条硬约束:
> ① **查源码按发布 tag 取,不按默认分支**。2026-10-13 实测:六个仓库的 latest
> tag 与默认分支 HEAD 全部不是同一 sha;opencode 默认分支是 `dev`、v2 发布线
> 在独立的 `2.0` 分支,pi 另有 `stable` 分支且与 latest tag 不同 sha——
> 默认分支上是"下一个版本"的代码,用它核对已发布版契约会错。
> ② 引用路径时写明 revision(见各节「调研基线」),上游改版后重新核对。
> ③ 商业/打包版与仓库可能有版本差(见各节「版本差异」),源码是查证工具而非
> 契约本身。

## 一、六个有源码的被管理工具

### 1. openai/codex ⭐（会话与 provider 知识最完整）

- **仓库**:Apache-2.0,Rust,~128.9k stars。latest release `rust-v0.162.1`。
  README 明示 CLI 本地运行;云端为 chatgpt.com/codex(与第三方 provider 并存,
  登录走 ChatGPT 账号或 API key,见 README authentication 段)。
- **调研基线**:默认分支 `main`(2026-10-13,`12cca49`);发布 tag
  `rust-v0.162.1`(`7a078f6`,从 main 打),关键文件已按该 tag 复核。
- **边界**:`codex-rs/` 是 ~150 crate 的完整 Rust workspace(CLI/TUI/exec/
  app-server 全开源);另有 npm 包装 `codex-cli/`、官方 `sdk/`(TS/Python)、
  官方文档 `docs/`(config.md、example-config.md、skills.md 等)、
  `codex-rs/app-server-protocol/`(JSON-RPC,可生成 JSON Schema/TS)。
- **会话转录**:`$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<thread_id>.jsonl`
  (`codex-rs/rollout/src/lib.rs:86` `SESSIONS_SUBDIR`;路径与文件名
  `recorder.rs` `precompute_new_rollout_path`、`rollout_file_name.rs`)。
  每行 `RolloutLine { timestamp, ordinal, ...item }`,item 以 `type` 区分
  (`session_meta/turn_context/event_msg/response_item/...`,
  `codex-rs/history/src/lib.rs:215`、`rollout_payload.rs:30`)。
  - token:`event_msg` → `payload.type == "token_count"` →
    `info.total_token_usage{cached_input_tokens, cache_write_input_tokens, ...}`
    (`codex-rs/protocol/src/protocol.rs:2386-2466`),reins 按累计值差分;
  - 模型:`turn_context.payload.model`(`protocol.rs:3440`),reins 做
    「最近一次声明」状态机;
  - 另有 `token_usage_record` 行(`protocol.rs:2386`)可作未来校验源。
- **provider 配置**:`$CODEX_HOME/config.toml`(`codex-rs/config/src/lib.rs:52`),
  `model_providers: HashMap`(`config_toml.rs:332`)、`wire_api`
  (`model-provider-info/src/lib.rs:107/166`;`"chat"` 写法已移除)、
  `model_catalog_json`(`config_toml.rs:406`)、`model_reasoning_effort`
  (`:398`);API key 经 `env_key` 指向的环境变量读取(`:481`)。
  内置 provider 仅 openai/bedrock/ollama/lmstudio 等(`:660`)。
- **目录重定向**:`CODEX_HOME`(`codex-rs/utils/home-dir/src/lib.rs:6-16`);
  另有 `CODEX_OSS_BASE_URL`。
- **版本差异**:release 与仓库同步(二进制即由此构建);变化点集中在 crate
  划分与 `wire_api` 值域,升级时按上面路径复核。
- **reins 接入**:reader `session/codex.rs`、adapter `providers/apps/codex.rs`、
  契约 `aidocs/context/2026-09-25-providers-contract-verification.md`
  (补充三/五/六)。

### 2. earendil-works/pi ⭐（schema 与文档最自解释）

- **仓库**:MIT,TypeScript,~114.7k stars。latest release `v1.2.0`。
- **调研基线**:默认分支 `main`(2026-10-13,`7d75a0e`);发布 tag `v1.2.0`
  (`06a4669`);另有 `stable` 分支(`abe508e`,Nix 渠道引用),**与 latest tag
  不同 sha**,两者关系 unverified——核对已发布行为用 tag,`stable` 只作参考。
- **边界**:pnpm monorepo,按能力分包——`packages/ai`(pi-ai,统一多 provider
  LLM API)、`packages/agent`(pi-agent-core)、`packages/coding-agent`
  (bin `pi`,含 `docs/` 与 `schemas/`)、`packages/tui`(差分渲染 TUI);
  另有 chord/durable/codemode/mcp/env 等组件包。
- **会话转录**:`~/.pi/agent/sessions/<cwd 编码目录>/<timestamp>_<session-id>.jsonl`
  (明文 JSONL;目录名把 cwd 的 `/` 等替换为 `-`,`session-manager.ts`
  约 L587/L1079,根由 `config.ts` `getSessionsDir()` 给出)。首行
  `{"type":"session","version":3}`,entry 树形(id/parentId),类型含
  `message/model_change/thinking_level_change/usage/compaction`;usage 为
  `{input, output, cacheRead, cacheWrite, totalTokens, cost{...}}`
  (格式文档 `packages/coding-agent/docs/session-format.md`)。
- **provider 配置**:`~/.pi/agent/models.json`(自定义 provider/模型)、
  `settings.json`(全局;项目级 `<项目>/.pi/settings.json`)、
  `auth.json`(凭据);优先级 `--api-key` > auth.json > models.json `apiKey` >
  环境变量(`docs/models.md`)。**JSON Schema 随包发布**:
  `packages/coding-agent/schemas/*.schema.json`(由
  `scripts/generate-schemas.ts` 生成),这是最省事的字段核对面。
- **目录重定向**:`PI_CODING_AGENT_DIR`(默认 `~/.pi/agent`)、
  `PI_CODING_AGENT_SESSION_DIR`(`config.ts`;完整列表
  `docs/environment-variables.md`)。
- **版本差异**:未见商业版说明(pi.dev 承担文档站与 share viewer);以已安装
  CLI 的 `--version` 与 schemas 为准。
- **reins 接入**:reader `session/pi.rs`、adapter `providers/apps/pi.rs`、
  探索记录 `aidocs/context/2026-09-05-pi-session-source-review.md`、
  `2026-09-11-pi-data-storage-and-subagent-extension.md`。

### 3. xai-org/grok-build ⭐（用法文档随包发布）

- **仓库**:Apache-2.0,Rust,~27.3k stars。**无 tag/release**;根 `SOURCE_REV`
  记录同步点。README 明示:仓库是 `grok` CLI/TUI 与 agent runtime 的 Rust
  源码,**定期从 SpaceXAI monorepo 同步、不接受外部贡献** → 仓库可能滞后于
  官方安装脚本发布的版本。
- **调研基线**:唯一分支 `main`(无 tag 可选);`SOURCE_REV`(本次
  `559751fd`)是仓库内容对应的内部 commit。**没有发布 tag 意味着无法按 tag
  核对**,契约以 `grok --version` + 本机 `~/.grok/docs/user-guide/`(随包
  导出,版本自洽)为准。
- **边界**:`crates/codegen/` 下 ~120 个 crate(`xai-grok-pager` TUI、
  `xai-grok-shell` runtime、`xai-grok-config/models/sampler/session-events/
  session-search`、`xai-chat-state`、`xai-dirs`);根 `Cargo.toml` 自动生成。
- **会话转录**:`~/.grok/sessions/<cwd 编码>/<session-id>/`,会话目录是
  **一组文件**(官方文档 `crates/codegen/xai-grok-pager/docs/user-guide/
  17-sessions.md`):`summary.json`(索引元数据)、`updates.jsonl`(权威对话
  日志,驱动 `/resume`)、`chat_history.jsonl`、`events.jsonl`、`signals.json`
  (会话信号计数)、`subagents/`、`compaction_checkpoints/` 等;
  cwd 目录名编码在 `crates/codegen/xai-dirs/src/lib.rs:61`。
  - **实测版本差(2026-10-13,本机发行版)**:仓库文档未列 `usage.json`,但本机
    `~/.grok/sessions/**` 同时存在 `usage.json`(72 个会话)与 `signals.json`
    (73 个)——`usage.json` 是 reins 现行读取点:per-turn 增量
    (`turns[]` 带 `endedAt`、`modelUsage`),reins 按 `endedAt` 归桶;
    `signals.json` 是会话级计数(含 `contextTokensUsed`、`modelsUsed` 等)。
    两者的文件集随版本演进,改 reader 前先 `ls` 实机会话目录核对。
  - usage 字段:`UsageTotals{input_tokens, output_tokens, cached_read_tokens,
    cache_creation_tokens, reasoning_tokens}`
    (`crates/codegen/xai-chat-state/src/usage.rs:29-57`)。
- **provider 配置**:`$GROK_HOME/config.toml`(`xai-grok-config/src/loader.rs:90`;
  另有 managed_config.toml/requirements.toml 层级)。`[model.<id>]` 支持
  `api_backend = "chat_completions"|"responses"|"messages"`、`base_url`、
  `context_window`、`reasoning_efforts` 等——**全字段表在随包文档
  `docs/user-guide/26-config-reference.md:387-426`**,单模型指引
  `11-custom-models.md`;内置目录 `xai-grok-models/default_models.json`。
- **目录重定向**:`GROK_HOME`(`xai-dirs/src/lib.rs:62-81`);
  另有 `GROK_CONFIG`/`GROK_CONFIG_PATH` overlay(白名单键)。
- **版本差异**:同步仓库 vs 官方安装脚本产物,以 `grok --version` 实测为准;
  文档随包导出到 `~/.grok/docs/user-guide/`,可直接读本机版本对应的文档。
- **reins 接入**:reader `session/grokbuild.rs`、adapter
  `providers/apps/grokbuild.rs`、`aidocs/context/grokbuild-integration-analysis.md`、
  `aidocs/decisions/2026-10-12-grokbuild-index-cache.md`。

### 4. deepseek-ai/deepseek-harness（dsh）

- **仓库**:MIT,TypeScript,~248.4k stars。tag 最高 `dsh-v0.2.1-alpha.2`,
  README 自述 **developer preview**、明示会有破坏性变更(先读 SAFETY.md)。
  homepage deepseek.com/harness;开源版与托管版的关系 README 未说明(unverified)。
- **调研基线**:唯一分支 `master`(2026-10-13,`d743267`);发布 tag
  `dsh-v0.2.1-alpha.2`(`6851e49`),`SESSION_FORMAT_VERSION = 4` 已按该 tag
  复核通过。
- **边界**:pnpm monorepo,"Everything is a Plugin"(cordis patch 组合)。
  `apps/cli`(bin `dsh`)、`apps/desktop`+`desktop-host`(Electron)、
  `packages/boot/*`(profile 组合与 config schema)、`packages/core/*`、
  `packages/session/*`(session-persistence-jsonl + 格式迁移链)、
  `packages/storage/*`、`packages/credentials/*`、`packages/llm/*`
  (含 **llm-pi-ai:直接拿 pi 的 pi-ai 当多 provider 适配器**)。
- **会话转录**:`$DSH_HOME/sessions/--<cwd>--/<session-id>/session.vN.jsonl[.zstd]`
  (读写 `packages/session/session-persistence-jsonl/`;默认 **多帧 zstd**
  ——header 帧 + 每 append batch 一帧、各自 checksum;`compression: 'none'`
  则纯文本)。**代际文件**:`session.vN`,取数值最高代,历史代不可变;
  当前 `SESSION_FORMAT_VERSION = 4`
  (`packages/core/session/src/types.ts:89`)。
- **配置**:`$DSH_HOME/cordis.patch.yml`(home 层 patch,覆盖一切 profile)、
  `$DSH_HOME/profiles/<name>/{package.json, cordis.patch.yml}`;
  凭据 `$DSH_HOME/.credentials.yaml`(必须 0600、
  `packages/credentials/credentials-local/src/index.ts:61`)。
  provider 经 `llm-pi-ai` 的 `providers` dict 配置(route → `apiKeyEnv`/
  `baseURL`,`apiKeyEnv` 是凭据引用不是明文);
  权威字段目录是生成物 `docs/config-catalog.md`
  (`scripts/gen-config-catalog.ts`)。
- **目录重定向**:`DSH_HOME`(默认 `~/.dsh`;
  `packages/util/home-paths` 的 `resolveDshHome()`)。
- **版本差异**:alpha、破坏性变更预警;格式版本号显式存在(读文件前先认
  `vN`),reins 的 reader 已按该结构实现。
- **reins 接入**:reader `session/dsh.rs`、adapter `providers/apps/dsh.rs`、
  `aidocs/context/2026-10-07-dsh-session-storage-exploration.md`。

### 5. anomalyco/opencode

- **仓库**:MIT,TypeScript,~213.0k stars。latest release `v2.0.27`
  (并行发布线 v1.19.0、vscode-v1.0.0)。
- **调研基线**:默认分支是 **`dev`**(2026-10-13,`460c626`)——**不是发布
  分支**;v2 发布线在独立的 `2.0` 分支(`7a6ce05`),发布 tag `v2.0.27`
  (`1f4d07a`)打在 2.0 线上。已按 tag 复核:`database-path.ts`、`sql.ts`、
  `schema.gen.ts` 在 dev 与 v2.0.27 一致(抽查);结构上 dev 是"下一个版本",
  **核对已发布行为一律按 tag**。
- **边界**:`packages/core` 是运行时本体(session/database/provider/config/
  plugin),`packages/ai`、`packages/schema`、`packages/cli`、`packages/tui`,
  以及 `packages/desktop`(Electron)、`packages/enterprise`、
  `packages/console`(含 **zen** 网关路由)、`packages/web`(文档站)、
  `sdks/vscode`。商业面:opencode.ai 的 Zen/Enterprise(unverified:是否由
  该仓库部署)。
- **会话存储**:SQLite `~/.local/share/opencode/opencode.db`
  (`packages/cli/src/database-path.ts`:文件名 `OPENCODE_DB ?? "opencode.db"`;
  数据目录 `packages/util/src/global-roots.ts`:XDG_DATA_HOME || ~/.local/share)。
  v2 表 `session_v2`/`session_message`(DDL 生成物
  `packages/core/src/database/schema.gen.ts`;drizzle 定义
  `packages/core/src/session/sql.ts`);`session_message.data` 是 JSON,
  assistant 消息带 `tokens: {input, output, reasoning, cache:{read, write}}`
  (`packages/schema/src/session-message.ts`、`token-usage.ts`)——reins 的
  usage SQL 直接对这几个路径 `json_extract`。迁移文件约 60 个
  (`packages/core/src/database/migration/*`),**字段漂移先查迁移链**。
- **provider 配置**:`opencode.json`/`opencode.jsonc`
  (`packages/core/src/config/discovery.ts`;含系统目录与 macOS MDM);
  对外 JSON Schema 为 https://opencode.ai/config.json。`providers` 段内部
  schema:`packages/schema/src/config/provider.ts`(含 `package` 字段——
  reins 按协议选原生运行时包的依据)。内置 provider 插件
  `packages/core/src/plugin/provider/*.ts`;模型目录来自 models.dev
  (`packages/core/src/models-dev.ts` + 快照)。
- **目录重定向**:无目录重定向变量;注意 **Windows 不认 XDG_CONFIG_HOME**
  (reins 实测,见 `environment.rs` 注释)。
- **reins 接入**:reader `session/opencode.rs`、adapter
  `providers/apps/opencode.rs`、契约文档处置结果 6/11/12/16。

### 6. zai-org/ZCode ⭐（provider 规则体系原生开源）

- **仓库**:Apache-2.0,TypeScript,~7.7k stars。latest release `v3.15.1`
  (根 package.json 同版;`apps/zcode-cli` 版本线独立,当时 0.16.9)。
  **桌面端与 CLI 同一仓库**:`packages/desktop` + `apps/zcode-cli/`
  (README 明说"本仓库包含客户端、后端服务、共享 UI,以及 Agent CLI 与运行时
  源码")。
- **调研基线**:默认分支 `main`(2026-10-13,`aac4755`);发布 tag `v3.15.1`
  (`9ab8a64`,main 线)。**仓库 tag 与已安装版同源已实证**:tag 上的
  `config/provider/zcode-builtin.json` `revision = 31`,与本机 app 包内
  该文件完全一致(2026-10-13);容器内两处一致时,契约核对优先用仓库 tag
  (可读、可 diff)。
- **边界**:`apps/zcode-cli/` 是 Agent CLI/runtime(独立 pnpm workspace,
  core/bootstrap/adapters/…)；根 `packages/` 是桌面与服务共享层——
  `provider`(配置规则与解析)、`provider-node`(内置配置物化/远端同步)、
  `shared`(model-config zod)、`model-option-map`(受限 CEL)、`ui`/`web` 等。
- **会话存储**:SQLite `~/.zcode/cli/db/db.sqlite`
  (`apps/zcode-cli/packages/adapters/src/storage/session-store/paths.ts`
  `getDefaultSessionDbPath()`;`ZCODE_DATA_BASE_DIR` 可改基目录)。
  用量表 **`turn_usage` / `model_usage` / `tool_usage`**
  (建表 `.../session-store/migrations.ts` 的 `0010_usage_observability`)：
  两者都带 `started_at`(毫秒 epoch)、`input/output/reasoning_tokens`、
  `cache_creation_input_tokens/cache_read_input_tokens`;`model_usage` 另有
  `provider_id`/`model_id`/`query_source`(reins 的模型维度用它)。
  写入方 `.../session-store/repositories/usage.ts`。
- **provider 配置**:磁盘落点 `~/.zcode/v2/provider_config.json`
  (本机运行时实测;仓库内读写模块 `packages/provider-node/src/
  personal-provider-config-repository.ts` + `provider-config-file-codec.ts`,
  文件路径由组合根注入,字面常量位置 unverified)。
  - 模型级 zod schema:**`packages/shared/src/model-config.ts`** ——
    `optionSpecs.reasoningLevel = {values: string[], map: string}` 与
    `optionSpecs.maxOutputTokens = {max, map}`(稀疏 overlay,可独立继承);
    `map` 是**受限 CEL 表达式**(只读同名变量,返回 RFC 7396 merge patch),
    编译实现在 `packages/model-option-map/`;
    `properties.contextWindow` / `properties.inputFormat.supportsImage` 等。
  - 规则体系:`packages/provider/src/config/rule-data-schema.ts`
    (modelMatch/modelApiMatch/providerSiteMatch/templateModel 等)与
    `model-config.ts`(overlay 与 resolve 顺序);
    **内置兜底**:`config/provider/zcode-builtin.json` 的 `modelRules[".*"]`
    给 `values:["disabled","enabled"]`,`modelApiRules` 三条按 `api.type`
    的 `.*` 兜底只给 `map`(anthropic-messages/chat-completions/responses
    各自的协议字段拼法)——**reins 只写 values、map 继承兜底**的依据。
  - 手动规则互斥:`manualProviderModelRules` 与 `providerModelRules` 同
    (provider, model) 不能并存(superRefine),删除 provider 时两类一起清
    (`deleteExactForProvider`)。
- **版本差异**:仓库与桌面版同源,但**平时用的是打包发行版**——内置
  `config/provider/zcode-builtin.json` 带 `revision` 号,可能领先或滞后仓库
  (2026-10-13 实测两者一致,均为 revision 31)。核对顺序:先比 revision,
  一致时用仓库 tag(可读、可 diff);不一致时以「本机 app 包内置配置 +
  运行时落盘文件」为准(bundle 反编译仅作补充,例:
  `/Applications/ZCode.app/Contents/Resources/glm/zcode.cjs`)。
- **reins 接入**:reader `session/zcode.rs`、adapter
  `providers/apps/zcode.rs`、契约
  `aidocs/context/2026-09-25-providers-contract-verification.md` 补充(七)、
  `aidocs/context/2026-09-29-zcode-session-storage-exploration.md`。

## 二、Claude Code:无官方源码

- 官方未开源;网上流传的「泄露源码」**不作依据**(合法性存疑、版本不可追踪、
  与已安装版本无对应关系),默认按无源码处理。
- 契约来源只有两条:官方文档 + 本机实测。已核对的落点(见契约文档处置结果
  10/14/15):`$CLAUDE_CONFIG_DIR/settings.json` 的 `env.ANTHROPIC_BASE_URL`/
  `ANTHROPIC_AUTH_TOKEN`(旧 `ANTHROPIC_API_KEY` 只读识别)、
  `CLAUDE_CONFIG_DIR` 重定向、`CLAUDE_CODE_MAX_CONTEXT_TOKENS`/
  `CLAUDE_CODE_MAX_OUTPUT_TOKENS`。
- 会话转录格式靠实测维护:reader `session/claude_code.rs`、
  adapter `providers/apps/claude.rs`。

## 三、横向对照表

| 工具 | 语言/许可证 | 会话存储 | provider 配置落点 | 目录重定向 | 默认分支 / 发布来源 |
|---|---|---|---|---|---|
| codex | Rust / Apache-2.0 | `$CODEX_HOME/sessions/**/rollout-*.jsonl` | `$CODEX_HOME/config.toml` | `CODEX_HOME` | `main` / tag 打 main |
| claude | (无源码) | `$CLAUDE_CONFIG_DIR/projects/**.jsonl`(实测) | `settings.json` 的 env(实测) | `CLAUDE_CONFIG_DIR` | — |
| pi | TS / MIT | `~/.pi/agent/sessions/**/*.jsonl` | `~/.pi/agent/{models,settings,auth}.json` | `PI_CODING_AGENT_DIR` 等 | `main` / tag 打 main;另有 `stable`(≠latest tag) |
| grok-build | Rust / Apache-2.0 | `~/.grok/sessions/<cwd>/<id>/`(多文件,`updates.jsonl` 权威) | `$GROK_HOME/config.toml` `[model.<id>]` | `GROK_HOME` | `main` / **无 tag**,按 `SOURCE_REV` + 随包文档 |
| dsh | TS / MIT | `$DSH_HOME/sessions/--<cwd>--/<id>/session.vN.jsonl[.zstd]` | `$DSH_HOME/cordis.patch.yml` + `profiles/*` | `DSH_HOME` | `master` / tag 打 master |
| opencode | TS / MIT | `~/.local/share/opencode/opencode.db`(SQLite) | `opencode.json(c)` | 无(Windows 不认 XDG,实测) | **默认 `dev`** / v2 发布线在独立 `2.0` 分支 |
| zcode | TS / Apache-2.0 | `~/.zcode/cli/db/db.sqlite`(SQLite) | `~/.zcode/v2/provider_config.json` | 无(有 `ZCODE_DATA_BASE_DIR`,非目录重定向) | `main` / tag 打 main;tag 与已安装包内置配置一致(实证) |

## 四、源码怎么用（用法纪律）

1. **先定版本再查,按发布 tag 取源码**:先确认本机工具的版本
   (`--version`、release tag、`SOURCE_REV`),再 checkout 对应 tag 核对;
   **默认分支不等于发布版**——opencode 默认 `dev`(v2 发布线在 `2.0` 分支)、
   pi 的 `stable` 与 latest tag 不同 sha、grok-build 无 tag 只能按 `SOURCE_REV`
   和随包文档。无 tag 可用的(grok-build)以本机版本产物为准。
2. **查证路径**:会话/用量格式 → 存储模块与迁移文件;provider 配置 →
   schema 定义文件(zod/JSON Schema)与内置默认值目录;CLI 行为 →
   文档目录 + 对应实现。都能在本文各节找到入口。
3. **结论要沉淀**:源码查证的结论写进 `aidocs/`(格式契约、字段口径),Ref.md
   只维护「去哪查」,不重复字段表;字段级细节以 aidocs 与代码为准。
4. **反编译只作补充**:打包产物(bundle/二进制)用于核对「运行时与仓库是否有
   版本差」,不作为契约定义;契约定义必须在源码或实测中找到。
5. **不抄代码**:语言与结构均不同;只借 schema 形状、字段口径、算法思路。

## 五、本次调研未闭环项

- codex:是否存在 `docs/` 之外的独立 schema 产物目录(unverified)。
- pi:`stable` 分支与 latest tag 的关系(实测不同 sha,谁先谁后 unverified)。
- grok-build:`signals.json` 写入点与逐字段定义、`events.jsonl` 写入调用点、
  `[model.<id>]` 的 Rust 反序列化结构位置(unverified);**无 tag**,
  仓库与已安装版本的对应关系只能靠 `SOURCE_REV` + 随包文档比对;
  仓库文档与本机发行版的会话文件集不一致(见上节,已实测记录)。
- dsh:会话事件的 usage 子字段名与事件时间戳字段名的源码位置;开源版与
  deepseek.com/harness 托管版的关系(unverified)。
- opencode:`2.0` 发布分支与 `dev` 的差异规模未度量(只抽查了 3 个关键文件
  一致);`packages/core/schema.json` 与线上 `config.json` 是否逐字段一致;
  桌面端 release 资产与 tag 对应关系(unverified)。
- ZCode:`provider_config.json` 字面文件名在仓库中的常量位置;tasksDatabase
  文件位置;zcode.z.ai 与 release 资产的对应关系(unverified)。
