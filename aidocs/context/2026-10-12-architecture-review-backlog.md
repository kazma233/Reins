# 架构评审待办清单

- 日期：2026-10-12
- 状态：待办（按建议顺序排列）

来源：2026-10-11 的架构加深评审（workspace / session / providers / 前端四路扫描 + ADR 交叉验证）。已完成项见文末；以下是剩余候选。文中行号为评审时快照，实施前以当前代码为准。

## 队列

### 1. C10 · 工具配置路径与 env 重定向：三处知识、两域口径已分叉

- 域：providers / workspace / agents；强度：Worth exploring（有真实咬人风险）
- 问题：「某工具全局配置在哪、认不认 env 重定向」存在三份——`agents.rs` 的 GlobalLayout、providers 的 ToolEnv、tests 的路径助手；且 CODEX_HOME / CLAUDE_CONFIG_DIR / DSH_HOME 只被 providers 承认。用户设置后，workspace 的 MCP 分发写 `~/.codex/config.toml`，providers 的模型配置写 `$CODEX_HOME/config.toml`——同一个文件两个域解析到两个位置。
- 方向：`GlobalRoot` 补齐 env 变体（解析走 `support::fs` 单一口径，接受可注入 env 视图保 ToolEnv 测试性），providers 路径函数从清单派生，测试助手改调清单。
- 前置：先定性「workspace 不认 CODEX_HOME」是知识漂移还是隐含决策（无 ADR 记载，大概率漂移）；`agents.rs`「只收静态知识不收行为」的模块定位需以增补方式修订。
- 证据：`src-tauri/src/agents.rs:102-260`（GlobalRoot 仅 Home/GrokHome/PiAgentDir）、`providers/apps/mod.rs:521-604`（ToolEnv）、`tests/providers.rs:178-230`（第三份拷贝）。

### 2. C1 · skill 分发的「target 安装面」收成深模块

- 域：workspace；强度：Strong
- 问题：「把一个 skill 装进一个 target」的物理语义没有接缝。三条平行遍历（sync `skills.rs:327-422`、conflicts 预览 `458-496`、remove `1101-1175`）各自携带 skip 规则（两份且行为刻意不同：`379-385` vs `470-475`）、落点计算（`389` vs `479`）、链接分类与路径规范化；同一个 destination 状态机被渲染成 4 种输出形状（`191-242 / 498-543 / 693-733 / 737-785`）。历史 bug（两侧规范化不一致导致 /var → /private/var 链接删不掉，`skills.rs:1106` 注释）恰好落在重复处。
- 方向：安装 / 预览冲突 / 枚举链接 / 移除四个动作收进一个以 target + 来源根为输入的模块；skip、落点、分类状态机、`normalize_link_path` 全部内化，输出统一 destination 枚举，文案由调用方渲染。
- 收益：落点 / skip 规则变化只改一处；单 target 最小夹具可测（现状须手拼三件套 config，`tests/workspace.rs:17-53`）。

### 3. C2 · target 的寻址与枚举缺一个接缝

- 域：workspace；强度：Strong
- 问题：复合 id（`project:agent`）的格式知识 7 份拷贝（构造 3、枚举 4：`skills.rs:108-118, 115, 1111-1121`、`mcps.rs:643-659`、`inspect.rs:95-116, 107`）；「target 不可用」四种表现（`mcps.rs:49/83/113` 三行逐字 bail、warning、静默 skip、状态呈现）。
- 方向：`ResolvedManagerConfig` 提供唯一的 target 枚举 / 寻址入口（id ↔ target+project 双向、enabled 判定内化）；各操作只声明不可用时的呈现方式。纯内部机械改动，不动 Tauri 契约面。

### 4. C4 · pi 的手搓缓存机械迁引擎部件

- 域：session；强度：Strong
- 问题：pi 保留约 100 行私房机械（3 个 static + 锁助手 `pi.rs:54-72`、index 缓存 + 指纹 `228-284`、三级摘要回填 `340-373`），引擎里已有同构部件（`reader_engine.rs:286-342` SummaryMemCache、`358-361` CachedIndex）；指纹函数与 `dsh.rs:823-837` 逐字孪生。pi 测试还被进程 HOME + 全局锁钉死（18 处 TestEnvGuard），不能并行。
- 方向：把 `DualHalfCache` 的借用模式推广到指纹目录缓存与 SummaryMemCache，pi 按实例持有部件；HOME 重开语义逐字保留。
- ADR：构成对 `2026-10-08-reader-engine.md` 决策 6「pi 摘要缓存保持现状」的修订，应记录而非默默改。

### 5. C14 · workspace/model.ts 收深

- 域：workspace 前端；强度：Worth exploring
- 问题：(a) `projectId:agentId` 拆分规则三处泄漏（`workspace/model.ts:26-34`、`McpPanel.vue:80-83`、`SyncTargetGroups.vue:51-53`），`groupTargetIds` 先格式化成展示文本再按 `/` 反解析；(b) mcp 表单 parse/format/validate 三件套困在 `useMcpMutations.ts`（68-135, 241-262, 284-310）私有函数里，行级校验零直接测试——对照 providers/model.ts:397-511 + 533 行测试的已验证形态；(c) agent 清单前端残留第六份拷贝 `AVAILABLE_PROJECT_AGENTS`（`model.ts:262`，不锚定生成类型）与死导出 `BuiltinTargetPresetId`（71-78）。
- 方向：先做 (b)（照抄 providers 模板，收益/风险比最高）；(a) `parseTargetId` 单点；(c) Rust 侧 codegen 导出 AGENT_IDS 派生、死导出删除（不得手改生成文件）。

### 6. C3 · MCP writer 调用面收尾

- 域：workspace；强度：Worth exploring
- 问题：调用 writer 须维持 config_path 双源不变量（`mcps.rs:131-181` 四个薄壳同条件三种 None 行为：52 bail / 86 bail / 170 noop）；读取侧 `inferred_target_layout`（335-364）绕过 parse 用 builtin defaults 重建 target——同一份配置存在三种解释；transport 校验三层三文案（`mod.rs:547-558`、`mcps.rs:720-731`、`mcp_formats.rs` 八处防御 bail）；inspect 自建 `McpEntryCache`（`inspect.rs:15-18, 185-206`）补逐次 IO。
- 方向：resolved target 成为唯一携带写入地址的输入，无地址 / defaults 兜底 / transport 校验各收一处；读侧归一化挂进同一接缝。与 `context/2026-09-15-mcp-config-mapping.md` 的适配器契约先对齐，保持「mcps 持引擎、mcp_formats 持格式知识」分工。

### 7. C6 · 删除侧线的散落机械

- 域：session；强度：Worth exploring（两个 Windows 真 bug 已修，结构问题仍在）
- 问题：官方 CLI 删除运行器 ×3（`codex.rs:207-234`、`opencode.rs:312-334`、`grokbuild.rs:979-1002`）；`prune_empty_parents` ×3 且已漂移（`claude_code.rs:670` 缺 `current != root` 保护，可能删掉 `~/.claude/projects` 根——`pi.rs:1152` 与 `grokbuild.rs:1045` 有保护）；删除计划从展示字符串反推（`delete.rs:31-46` 从 overview.agents 推 id；grokbuild / claude 的字符串手术已在 2026-10-11 改为 Path 组件推导，但 plan 输入仍是渲染 DTO）。
- 方向：执行侧收「CLI 运行器 + prune」两个小件（prune 边界单点，claude 的分叉被迫显式决策）；预演侧让 plan 拿与执行侧同源的结构化成员数据。

### 8. C9 · reins 条目生命周期策略

- 域：providers；强度：Worth exploring
- 问题：移除策略（只删可识别条目、默认引用随删清空、无条目报错）六个适配器各写一套——报错文案 ×4 拷贝、默认引用清理六种写法（pi `286-331`、dsh `567-616` 文件内部又各抄一遍）、`classify_reins_entry` 的 base_url 相等口径迫使 grok 全量克隆视图（`grokbuild.rs:64-76`）、claude 旁路（`93-95`）。共享层只有数据构造器（`apps/mod.rs:388-485`）。
- 方向：先轻量收编（守卫 + 三句文案 + classify 比较口径钩子），观察剩余差异再决定是否上「注册树描述」。

### 9. C7 · grokbuild 的列表成本由域外缓存政策决定

- 域：session；强度：Speculative
- 问题：六家来源五家自管 freshness，唯 grokbuild 的成本取决于域外两个决定（`state/session_index.rs` 的 clear 调用点、`commands.rs:62` 的 refresh 默认值）；对 SessionIndexState 做删除测试，它主要是 grokbuild 放错位置的索引缓存。
- 方向：grokbuild 管自己的失效（哪怕只是自有缓存实例）。前置：先确认全量重建成本是否真实可感；若做，需在 reader-engine ADR 决策 6 补记「全量重建 ≠ 无缓存」。

### 10. C11 · codex apply 写序不变量测试

- 域：providers；强度：Speculative（小而实的顺手项）
- 问题：「目录文件必须排在 config.toml 之前写入」只活在 `apps/codex.rs:324-330` 的注释里，接口类型 `Vec<(PathBuf, String)>` 对顺序零承诺。
- 方向：一个测试断言 codex apply 产物的首个文件是 reins-models.json，把注释变成被守卫的事实。

## 小项（不单独立项，顺手做）

- providers 能力表 `ModelField::ReasoningLevels` 零适配器引用（`apps/mod.rs:66-79`）——2026-09-24 plan 声明的 unverified 项，验证后填充或删除。

## 已完成（2026-10-11 ~ 10-12，作为上下文保留）

- C8 方言表（`4994b5d` / `c31b33b`）：归一化枚举拼写单一来源 + roundtrip 守卫；修复 dsh `off→none` 映射文案活 bug。
- Windows 测试基线（`ea152bb` / `8371e1c`）：修复 claude delete plan 漏 meta.json、grokbuild delete plan 漏子会话目录两个真 bug；`GlobalRoot::resolve` 走 `user_home_dir`；快照比较统一分隔符口径。
- C13 token 展示口径（`6f088cb`）：收敛到 `sessions/token-usage.ts` 单一来源。
- C5 SQL 来源部件（`68b2961` / `b68ac69`）：父链分组（孤儿策略显式化）、用量口径双入口、tool 块装配收编；dsh 不入父链部件、孪生 usage 测试不强行收敛，见 `decisions/2026-10-12-sql-source-parts.md`。
- C12 分页状态机（`11a3596`）：`createPagedList` + `infinite-scroll` 助手，四个调用方塌缩为适配器，新增 14 个状态机直接测试。
- 范围裁决：子代理弹窗 mini-loader 是单次拉取形态、非分页，不收编。
