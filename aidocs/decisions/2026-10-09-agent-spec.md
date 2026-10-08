# 工具清单收敛为 agents 模块单一来源（AgentSpec）

- 日期：2026-10-09
- 状态：生效中
- 前置：`2026-10-08-session-source-registry.md`、`2026-10-08-reader-engine.md`、`2026-10-09-session-identity.md`（同一评审批次的姊妹决策）

## 背景

架构评审候选 6：「每个工具的 skill 目录、MCP 配置路径、config_type、产品名」这一个概念此前有五份拷贝——`workspace/targets.rs` 的 `builtin_target_defaults`（全局 7 条）与 `project_agent_defaults`（项目级 7 条，与全局非同构）、`workspace/mod.rs` 手写 YAML 模板、`providers/types.rs` 的 `PROVIDER_APPS + label()`、前端 `agent-labels.ts`（含修补 SourceApp id 与 AgentTargetId 不统一的别名表）。新增工具要改 3-5 处，漏改即静默分叉。根因是 2026-05-22 域红线没给「三域共享的业务概念」留位置（support 禁业务语义、三域互不依赖）。

## 决策

1. **新一级模块 `src-tauri/src/agents.rs`**：每工具一条 `AgentSpec`——规范键、产品名（跨层唯一一份）、跨域 id 映射（`source_app`/`target_id`/`provider_app`，**各域 wire id 保持现状**，映射进 spec，不强求统一标识）、层级无关的 MCP 布局（prefix + config_type）、全局布局、项目布局（非同构如实：claude 项目级 `.mcp.json`、dsh 项目级无 MCP）、模板形态（`Standard{mcp_enabled}`/`FollowEnv`/`Absent`）。只收静态描述，不吸收任何行为。
2. **域红线修订**（2026-05-22 文档）：session/workspace/providers 两两互不依赖不变；新增「一级模块 `agents`（工具静态清单）三域均可依赖，自身不依赖任何业务域」。这是对原红线的定向重开，摩擦证据是真实的五份拷贝。
3. **派生改造**：两张 defaults 表改为遍历 AGENTS 构造；手写 YAML 模板改为「构造 TemplateTarget 结构 + 渲染器」（serde_yaml 无法逐字节复现注释/空行/空串引号，故 config_type 键经 serde 取 wire 串、其余渲染器输出）——模板全文由快照测试逐字节锁定；PROVIDER_APPS label 与 providers 错误串工具名经 spec。
4. **候选 7 一并落地**：`AppAdapter` 增 `config_paths()` 能力声明（各 adapter 实现），`providers/commands.rs` 的六臂 match 删除；dsh 的 inspect 展示路径集（3 条，含 workspace 域管的 patch 文件）与 config_paths()（2 条，providers 域管理文件）语义不同，各守其位。
5. **前端生成物**：`src/shared/lib/agent-labels.ts` **路径不变**变生成物（AGENTS.md 的既有引用保持准确），由 `cargo test export_bindings_agent_labels`（`pnpm codegen` 同一过滤词触发）从 AGENTS 表生成 `AGENT_LABELS` + `AGENT_ID_ALIASES`（wire id 差异自动导出）；`agentDisplayName` 别名函数退役，三个消费点（sessions 的 source-app、workspace 的 model、providers 的 model）改经生成映射。

## 生效约束

- 工具静态知识（目录/路径/格式类型/产品名/模板形态）只存在于 `agents.rs` 一条 AgentSpec；各域 defaults 表、模板、label 不得再出现手写副本。
- 新增工具 = agents.rs 一条 AgentSpec + 各域各自的行为实现（reader/adapter/writer）。
- 规范键取 AgentTargetId 公共形式（唯一 wire 例外 claude_code 由 `source_app` 字段承载），不引入第三种键。
- 前端产品名/别名只从生成物取，生成物随 `pnpm codegen` 提交（幂等）。

## 验证

- 八张派生等价快照测试**先对现状代码跑绿**（builtin/project defaults 逐字段、模板全文逐字节、provider label 集合、config_paths 六臂输出），派生改造后未改一字全部保持绿——行为等价的主证据。
- `cargo fmt --check` / `cargo check --all-targets` 0/0；`cargo test` 383 通过 / 0 失败 / 2 忽略（另见下）；`pnpm codegen` 幂等；`pnpm test` 167 通过（agentDisplayName 用例随函数退役）；`pnpm build` 通过。
- 净 -40 行（22 改 + 2 新增，+267/−307）。
- **unverified**：一次未复现的测试闪失（本批与实施代理各遇一次，均在冷启动首轮、其后 20+ 轮含单线程全绿，未能捕获用例名）——按证据边界记录，待再次出现时定位。

## 相邻遗留（记录不处理）

- session 域 reader 的 display_name/锁前缀（"Claude Code" 等）仍是引擎契约文本，按「不过度强求统一」未并入 spec；若未来要收敛，经 AgentSpec.label 派生是现成路径。
- `workspace/inspect.rs:144` 有一处读取侧 `McpConfigType` key 归类判断（候选 3 报告的相邻项），可后续收敛为 writer 的 `entry_key_for` 声明。
