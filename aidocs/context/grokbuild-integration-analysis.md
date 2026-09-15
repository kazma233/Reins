# Grok Build 接入分析

## 实施与验收更新

以下原始分析保留研究时的证据边界。本节记录随后实施的结果，优先于下文的待验证状态。

- 已实现全局/项目 `grokbuild` target、Skill 分发注册、Grok TOML MCP schema，支持 `GROK_HOME`，旧配置不自动迁移。
- 隔离 Grok 1.0.30 检查确认软链接 Skill 可被发现；MCP timeout 要求整数秒，非整秒毫秒值明确拒绝。
- 用户授权只读检查真实 sessions 后，确认布局为 `sessions/<bucket>/<id>/summary.json`，格式版本 1；cwd 从 `summary.info.cwd` 读取，不推测 bucket 编码。
- 已实现会话来源、摘要列表、消息与事件分页、恢复命令。消息以 `chat_history.jsonl` 为源，不重复拼接流式正文；不暴露 encrypted_content，过滤 synthetic system_reminder。
- 工具结果失败标记从 events/updates 提取。转出到 Claude/Pi 的合成读回测试通过；Codex/OpenCode 转出预览会提示失败标记不能保留。
- 不支持导入到 Grok，也不提供 Grok 删除操作。
- 子代理聚合已实现：只按 `subagents/<id>/meta.json` 确认归属，列表把已确认子会话折入主会话（标题 `(+N subagents)`），overview.agents 列出父子，主时间线只放 marker 入口，打开入口时按 child id 读取 marker + 子会话正文；meta 声明但子会话缺失时父会话照常可读、入口打开明确报错；未声明父会话的子会话、以及未验证的嵌套子代理保留为独立条目，不推测层级；重复归属、attempt 不匹配、自我声明显式报错。
- transcript_path 统一为规范路径（macOS `/var` 与 `/private/var` 只保留一个拼写），折叠后的子会话路径仍属于同一 family。
- 主 agent 独立运行：Rust 191 项通过（4 项原有集成测试跳过）、前端 46 项通过，类型检查与构建通过。真实目录只读验收：10 个会话（已折叠）、12 个 agent 条目、191 条子代理消息，只输出计数。
- `grokbuild_local_store_readonly_acceptance` 默认 ignored，只能显式执行，不在普通测试中读取真实会话。
- 尚未验证桌面 UI、真实 MCP 连接和 CLI 恢复；消息分页每次扫描相关日志，尚无专门性能优化。事件 tab 仍只展示父会话事件，未拼接子会话事件（无法可靠交错时间）。

## 结论与范围

目标：Reins 的会话来源、全局 targets、项目 agents、Skill 分发和 MCP 管理支持 `grokbuild`。本次只分析，不修改应用代码，不安装或升级 Grok，不读取真实凭据或会话正文，不执行推理、导入、删除。

统一内部 ID 建议为 `grokbuild`，显示名称为 `Grok Build`，实际可执行文件为 `grok`。三者不要混用。

Skill 和 MCP 路径有官方契约，可以开始实现。会话目录与恢复、删除命令已确认，但磁盘 schema 尚未确认，完整会话接入需要补充脱敏样本。不要把 Grok 对 Claude 扩展的兼容声明当成会话格式兼容声明。

## Subagent 证据与待确认项

最新真实样本确认主会话有两个并行 general-purpose 子代理，均 completed。结构为父会话 `subagents/<subagent_id>/meta.json` 和 `output.json`；子会话正文独立存于同级会话目录。meta 的 parent_session_id、child_session_id、attempt_id 与子 summary 和父 updates 启停事件交叉一致。子 summary.session_kind 为 subagent，但子 events.session_relationship 仍为 primary，不能作为父子判断依据。spawn_subagent 工具结果为非 JSON 文本，不作为关系解析源。

本阶段允许的行为：以结构化 meta 确认父子归属、列表折入主会话、子代理入口及独立详情。主消息保持原顺序，子消息保持各自原顺序，不伪造跨会话交错时间。

已按上述规则实现并通过合成 fixture 与真实目录只读验收。嵌套子代理（父自身也是子代理）当前保留为独立条目，不折叠；重试、fork、运行中/失败/取消状态的完整枚举仍未验证。

仍待确认：

- 重试/恢复：subagent_id 与 attempt_id 的生命周期、覆盖或多版本存储方式。
- 嵌套子代理：多级关系归属和存储规则。
- 执行中/失败/取消：完整状态枚举、可缺字段、元数据和子会话文件的写入顺序。
- 孤立关联：实际产品删除、异步写入或迁移造成父/子缺失的行为。当前仅可明确提示不可读取，不能猜归属或静默丢掉独立会话。
- Fork：是否复制已有子代理关系、旧关系是否仍有效。
- 消息时间：chat_history 没有逐条时间戳，不能准确交错父子消息。

删除 Grok 会话和导入到 Grok 仍不开放。上述未知项不阻塞普通已完成子代理的只读展示。

## 一手来源

- [Overview](https://docs.x.ai/build/overview)：产品与 CLI 入口。
- [Skills](https://docs.x.ai/build/features/skills-plugins-marketplaces.md)：Skill 路径、frontmatter、兼容扫描。
- [MCP Servers](https://docs.x.ai/build/features/mcp-servers.md)：MCP 路径、字段、作用域和优先级。
- [Settings Reference](https://docs.x.ai/build/settings/reference.md)：`GROK_HOME`、完整 MCP 字段、项目配置有效范围。
- [Sessions](https://docs.x.ai/build/features/sessions.md)：持久化目录、恢复、fork、删除、Markdown 导出。
- [CLI Reference](https://docs.x.ai/build/cli/reference.md)：CLI 命令及参数。
- [Headless](https://docs.x.ai/build/cli/headless-scripting.md)：流式输出和 ACP，不能据此推断磁盘格式。
- [Subagents](https://docs.x.ai/build/features/subagents.md)：子代理是独立子会话，但未描述磁盘父子关系字段。

官方文档通过 `.md` 端点直接获取。研究委派工具报错 `timeoutHandle is not defined`，没有采用任何不完整子 agent 输出；以下结论由主 agent 直接核实。

## 已确认的外部契约

| 对象 | 全局 | 项目 |
| --- | --- | --- |
| Skills | `$GROK_HOME/skills`，默认 `~/.grok/skills` | `<project>/.grok/skills` |
| MCP | `$GROK_HOME/config.toml`，默认 `~/.grok/config.toml` | `<project>/.grok/config.toml` |
| MCP 节点 | `mcp_servers` | `mcp_servers` |
| 会话 | `$GROK_HOME/sessions`，默认 `~/.grok/sessions` | 按工作目录区分，具体磁盘编码 unverified |

来源：Skills、MCP Servers、Settings Reference、Sessions。

- 项目 Skills 从 cwd 向上扫描到仓库根目录。文档没有给出同名 Skill 的完整胜出规则，软链接扫描行为也仍为 unverified。
- Grok 还读取 `~/.agents/skills`、Claude 兼容目录及插件 Skills；这些是 Grok 的额外发现来源，不应作为 Reins 的 Grok 专属默认写入目录。
- 项目 MCP 同名项整体替换用户项，不是字段级合并。兼容读取的 Claude/Cursor MCP 低于原生配置优先级。
- 项目 config 只贡献 `mcp_servers`、`plugins`、`permission`。不要向项目 config 写 `[skills] paths` 来实现项目 Skill 分发。
- MCP stdio 字段：`command`、`args`、`env`，另支持 `cwd`。
- MCP HTTP/SSE 字段：`url`、`headers`，另支持 `bearer_token_env_var`。
- 公共字段：`enabled`、`startup_timeout_sec`、`tool_timeout_sec`、`tool_timeouts`。
- `${VAR}` 和 `${VAR:-default}` 在加载时展开；Reins 应保留原文，不自行解析成秘密值。不得读取或管理 `mcp_credentials.json`。
- 恢复命令使用 `grok --cwd <cwd> --resume <id>`；`--session-id` 不是恢复参数。
- 官方单会话删除命令为 `grok sessions delete <id>`；Markdown 导出不是原生会话导入协议。

## 本机 CLI 证据与文档差异

只运行了可执行文件定位、版本、帮助命令，禁用了自动更新环境开关：

- `command -v grok`：`/Users/fanggeek/.grok/bin/grok`。
- `grok version`：`grok 1.0.30 (04b7ffed98c6)`。
- `grok sessions list --help`：只列出 limit/debug 等选项，没有列出 JSON 输出选项。不能据此设计依赖 `sessions list --json` 的方案。
- `grok sessions delete --help`：确认接受单个会话 ID。
- `grok import --help` 返回顶层帮助，命令列表没有 `import`。与 CLI Reference 的“从 Claude 导入”声明不一致，当前安装版本的官方导入支持为 unverified。
- 顶层帮助明确 `--session-id` 只命名新会话；Headless 文档表格写“Create or resume”，与 Sessions 文档和当前帮助冲突。接入使用明确的 `--resume`。

本次没有使用 `inspect` 打印用户配置，也没有读取实际会话正文。运行时 Skill 发现、MCP 加载、会话读写均未验收。

## 项目现状与改动边界

### 1. Targets 与项目 agents

当前全局 defaults 和项目 defaults 位于 `src-tauri/src/workspace/targets.rs`；前端预设与项目 agents 列表位于 `src/features/workspace/model.ts`。Skill/MCP 面板从 resolved targets 派生，不需要各复制一套 Grok 列表。

需要修改：

- `targets.rs`：全局及项目 defaults 增加 `grokbuild`。
- `workspace/mod.rs` 的 `default_config_template()`：新配置模板增加该 target。
- `workspace/model.ts`：`BuiltinTargetPresetId`、`BUILTIN_TARGET_PRESETS`、`AVAILABLE_PROJECT_AGENTS` 增加 `grokbuild`。
- 默认路径文档与 target 相关测试同步更新。

`parse_manager_config()` 只遍历已配置的 `raw.targets`，不会自动插入全部内置 defaults。因此修改模板不会使已有用户配置自动出现 Grok。建议老用户通过新增 target 预设显式添加；若要求升级后自动出现，需要另行确认迁移规则，不静默改用户配置。

全局路径解析要考虑 `GROK_HOME`：会话 reader 与后端 Grok 默认路径应共享这一真实根目录规则；前端预设不能在设置了自定义 HOME 时又写回硬编码路径。已有显式 target 路径仍以用户配置为准。具体实现限制在 Grok 路径解析与预设传递，不顺带改其他来源。

### 2. Skills

复用 `src-tauri/src/workspace/skills.rs` 的来源管理、安装、替换、卸载、软链接归属检查；不新增 Grok 专用 Skill 仓库，也不改写 `SKILL.md` frontmatter。

默认分发到 `.grok/skills`，避免写入 `.agents/skills` 时无意影响其他 agents。不要自动关掉 Grok 的 Claude/Cursor 兼容扫描。

重要语义：Reins 当前 target 安装状态描述的是目标目录分发状态，不等于 Grok 全部生效扩展。删除原生目录链接后，同名 Skill 仍可能从 Claude、`.agents` 或插件被发现；MCP 也可能在移除原生条目后重新暴露兼容来源条目。若要管理 Grok 的最终生效集合，是另一项功能，不能在本任务中默认扩展。

### 3. MCP

不能直接使用当前 `McpConfigType::Common` TOML 输出：

- `src-tauri/src/workspace/mcps.rs::desired_openai_toml_mcp()` 写远端 headers 为 `http_headers`。
- Grok Settings Reference 要求 `headers`，尚无证据表明支持 `http_headers` 别名。
- 通用 writer 把毫秒 timeout 转为浮点秒；Grok 文档只说秒，没有明确数字类型及小数接受规则，需验证，不能静默截断。

建议沿已有 `Common`/`OpenCode` 模式增加 `McpConfigType::GrokBuild`，序列化 ID 显式为 `grokbuild`。只增加 Grok TOML 条目适配，复用文件解析、节点定位、预览、应用、移除和读取流程。不通过 target 名称后缀猜 schema，因为全局、自定义名称、项目 target 均应由 config type 决定。

对应改动：`workspace/types.rs`、`workspace/mcps.rs` 中格式分发及 exhaustive match、前端格式说明及 `TargetCreateDialog.vue` 格式选项、生成的 TS 绑定。JSON 路径若与 Grok 格式组合不受支持，应明确报错。

映射已有字段即可：stdio 的 command/args/env，远端 url/headers，enabled，毫秒到秒 timeout。此次不扩展 cwd、OAuth、bearer_token_env_var、工具级超时等管理表单；环境变量引用可通过现有 headers/env 表达。

字段口径差异与后续改造方向（统一 MCP 配置动态映射到各 agent 字段）记录在 `aidocs/context/2026-09-15-mcp-config-mapping.md`。

### 4. 会话

会话来源是 `SourceApp` 枚举，不是 workspace target 配置。新增 target 不会自动增加会话来源。

- `session/model.rs`：增加 `GrokBuild`，显式 serde/TS 名为 `grokbuild`，补 FromStr/as_str。
- 新增 `session/grokbuild.rs`，实现 `SessionReader`：枚举、ID 定位、摘要、overview、消息和事件分页、detail。
- `session/mod.rs`：reader、缓存清理、删除分发。
- `session/catalog.rs`：来源检测和跨来源列表注册。
- 根据真实父子字段决定接入 `family_index.rs` / `family_timeline.rs`，复用公共聚合逻辑；不要按文件名猜父子关系，也不要把 fork 自动当子代理。
- 前端 `source-app.ts`、`model.ts`：名称、恢复命令、删除文案及命令预览；通过 codegen 更新类型。
- 恢复命令对 cwd 和 session ID 做 shell quoting。CLI 调用使用参数数组，删除复用现有执行边界并设置超时；先确认单会话与会话组删除语义，再接入 UI。

会话导入需要单独处理：当前 `session/mod.rs::exporter()` 对每个 SourceApp 都要求 exporter，`import.rs::assess_import()` 对大多数不同来源默认支持，而且 preview 即使不支持仍调用 planned_import_paths。

因此第一阶段若只提供 Grok 读取，必须直接调整这些调用点：未支持的 Grok 导入目标显式返回 unsupported、不请求写入路径、不进入执行；前端不将 Grok 加到可导入目标列表。不能为满足枚举分支而返回空路径、成功结果或生成猜测格式。

Grok 会话读取成功后，从 Grok 转出到现有目标可复用标准化消息模型，但须验证工具调用、结果和失败标记。导入到 Grok 则等待原生 schema 或当前版本可靠官方 importer 的证据。

## 实施顺序与验收

1. 补齐证据：准备不含秘密的 Grok 原生会话 fixture，包含普通对话、工具调用成功/失败、子代理、fork、压缩、重命名。记录 CLI 版本、目录结构、字段及辅助文件关系。验证 softlink 扫描及 MCP timeout 类型。
2. 完成全局/项目 target 与 Skill 路径注册，明确旧配置通过预设新增的行为。
3. 完成 Grok MCP 格式适配，验证 preview/apply/read/remove 一致性。
4. 完成会话 reader、分页、索引、缓存、恢复命令，按验证结果开放删除及跨来源转出。
5. 原生写入或官方 importer 获得验证后，再开放“导入到 Grok”。

自动化验收：

- 默认路径、自定义 GROK_HOME、显式路径优先、项目路径、旧配置不被覆盖。
- Skill 全局/项目安装替换卸载；卸载不删除源目录；同名冲突、断链遵循原行为。
- MCP stdio/HTTP/SSE、headers 键、enabled、变量引用原文、timeout 单位/类型；Codex 仍使用 http_headers。
- MCP 应用/移除保留无关配置语义，重复应用幂等，格式错误显式失败。
- 会话 summary/detail、分页不重不漏、跨来源排序、父子/fork关系、刷新失效、未知/损坏记录处理。
- Grok 不支持的导入目标在 preview 和 execute 都被阻断。
- `pnpm codegen`、`pnpm test`、`cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm build`。

运行时验收应在隔离 GROK_HOME 和测试项目进行：仅用合成配置及脱敏 fixture 验证 Skill/MCP 被真实 CLI 识别、会话可以恢复。`inspect --json` 输出可能含配置值，必须筛选非敏感结果；涉及连接、推理或删除前明确授权与范围。

本次未运行项目测试，因为没有实现代码；以上是后续验收标准，不是通过记录。
