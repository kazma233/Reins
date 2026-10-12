# 架构评审待办清单

- 日期：2026-10-12
- 状态：待办（第 1、2、3 项已完成；剩余按建议顺序排列：C11 → C14 → C4 → C3 → C6 → C9 → C7）
- 实施顺序的建议依据：C11 最小（一条断言把注释变成被守卫的事实）；C14 收益/风险比最高且清单自己这么排（mcp 表单照抄 providers 的已验证形态，行级校验目前零直接测试）；C4 最重但解掉它之后 session 域测试才可能并行（18 处 `TestEnvGuard` 把测试钉在进程 HOME 上）

来源：2026-10-11 的架构加深评审（workspace / session / providers / 前端四路扫描 + ADR 交叉验证）。已完成项见文末；以下是剩余候选。文中行号为评审时快照，实施前以当前代码为准。

## 队列

### 1. C10 · 工具配置路径与 env 重定向：三处知识、两域口径已分叉

- 域：providers / workspace / agents；强度：Worth exploring（有真实咬人风险）
- 状态：**已完成**（2026-10-12，见 `decisions/2026-10-12-global-root-env-redirect.md`）。剩余仅两个 unverified 点（claude 的 `CLAUDE_CONFIG_DIR` 语义）与 session 域的同类解析（已单列，不在 C10 范围）。
- 问题（修复前）：「某工具全局配置在哪、认不认 env 重定向」存在三份——`agents.rs` 的 GlobalLayout、providers 的 ToolEnv、tests 的路径助手；且 CODEX_HOME / CLAUDE_CONFIG_DIR / DSH_HOME 只被 providers 承认。用户设置后，workspace 的 MCP 分发写 `~/.codex/config.toml`，providers 的模型配置写 `$CODEX_HOME/config.toml`——同一个文件两个域解析到两个位置。
- 已做：重定向知识收进 `support::environment`（`Environment` 是进程环境的唯一读取点，`Redirect` 同时持变量名与默认目录）；`GlobalRoot` 补齐三个 env 变体且与模板渲染共用同一判据；providers 的路径函数从清单派生（`mcp_config_path(app)` 读 `mcp_config_path`），原 `ToolEnv` 删除；workspace 的环境挂在 store 上（`app()` 读环境、`at()` 空视图、`with_env()` 注入）；`tests/providers.rs` 的第三份路径助手删除，测试改调生产函数。
- 证据：`src-tauri/src/support/environment.rs`、`agents.rs`（GlobalRoot / GlobalLayout）、`workspace/config.rs`（store 持环境）、`providers/apps/mod.rs`（路径函数派生化）。

### 2. C1 · skill 分发的「target 安装面」收成深模块

- 域：workspace；强度：Strong
- 状态：**已完成**（2026-10-12，见 `decisions/2026-10-12-skill-install-surface.md`）。
- 问题（修复前）：「把一个 skill 装进一个 target」的物理语义没有接缝。三条平行遍历（sync、conflicts 预览、remove）各自携带 skip 规则（两份且行为刻意不同）、落点计算、链接分类与路径规范化；同一个 destination 状态机被渲染成 4 种输出形状。历史 bug（两侧规范化不一致导致 /var → /private/var 链接删不掉）恰好落在重复处。
- 已做：新增 `workspace/skill_install.rs`——`resolve_ids` / `managed_targets` 统一目标与 skip 判定，`SkillInstall` 提供 `apply` / `conflicts` / `remove_installed` / `remove_at` 四个动作，`DestinationState`（uninstalled / 指向本 skill / 指向同来源别的 skill / 指向别处 / 真实文件目录）是三态比较的唯一实现，`enumerate_links` / `normalize_link_path` / `relative_to_source_root` 内化；`skills.rs` 只留来源发现、git 缓存与编排，四条调用路径全部改为调用模块，重复的 `SkillAction`/`SkillActionKind` 与本地路径助手删除。
- 收益落地：单 target 最小夹具 5 个直接测试（`skill_install.rs` 内联模块），不再手拼整份 config.yaml。
- 证据：`src-tauri/src/workspace/skill_install.rs`、`workspace/skills.rs`（1326 行，较改造前少约 370 行）。

### 3. C2 · target 的寻址与枚举缺一个接缝 · 已完成

- 域：workspace；强度：Strong
- 落地：`TargetRef` 是复合地址的唯一定义（构造 + 拆分），`ResolvedManagerConfig`
  提供 `target` / `target_by_id` / `target_refs` / `managed_refs` / `skip_reason`
  五个入口；`TargetSkip` 从安装面搬到寻址层，调用方只决定表现方式（报错 /
  warning / 静默）。
- 实测修正了原清单的两处判断：(a) 「枚举 4 处」不是同一件事——inspect 的
  `TargetInspection`、`ProjectInspection`、前端配置视图、删 MCP 的两段循环输出
  形状各异，能被收编的是「(地址, target) 组合的产出」而不是遍历本身；(b)
  「enabled 判定内化」这条随启停废弃已不存在，真正需要内化的是「这个地址参不
  参与分发」这一条规则。
- 顺带修掉的三个真 bug 见下方「本轮修掉的真 bug」。
- 提交：`9fdfe82`（机械重构）、`803e523`（project id 字符集）、`21b4696`
  （项目 agent 落点回退）、`a79bd4d`（契约带上两段，前端不再拆地址）。

### 4. C4 · pi 的手搓缓存机械迁引擎部件 · 已完成

- 域：session；强度：Strong；完成于 2026-10-12
- 落地：`PiBackend` 从 unit struct 改为「static 前置 + 按 root 缓存实例」，形态照抄引擎的
  `FamilyReader`（`slot: Mutex<Option<(PathBuf, Arc<PiReader>)>>`，换根即整实例重建）；
  三个部件全部改用引擎实现：时间线 → `DualHalfCache`、索引 → 实例内
  `Mutex<Option<CachedIndex>>`（root 由实例表达，`source_key` 字段删除）、三级摘要 →
  `SummaryMemCache`。顺带把 `summary_cache` 的**进程级全局连接**整个删掉（`CONNECTION`
  静态、模块级 `load`/`store`/`clear_all`/`with_connection`/`cache_dir`），调用点全部改到
  实例级 `SummaryDb`（新增 `at_store_dir()` 与 `clear_all()`），跨进程持久化测试也改用实例 API。
- 验证：cargo test 441 passed（行为逐条对齐，既有 `session_cache` / `session_delete` /
  `pi` 用例全部原样通过）；fmt 与 clippy 保持基线 73。
- **未验证（如实记录）**：本轮曾尝试加一条「换根后不命中旧缓存」的测试，但它**在旧实现上
  同样通过**——时间线缓存按绝对路径做键，两个 home 的路径天然不同，本来就不会撞，所以该
  测试证明不了迁移的必要性，已按「不为不成立的主张留测试」删除。这次迁移的收益是结构性的
  （删掉进程级可变状态与重复机械），不是由一个失败用例驱动。
### 5. C14 · workspace/model.ts 收深（(b) 已完成）

- 域：workspace 前端；强度：Worth exploring；(b) 完成于 2026-10-12
- 问题：(a) `projectId:agentId` 拆分规则泄漏（实施 C2 后同步弹窗那处已随契约收掉，现存 `workspace/model.ts:26-34` 与 `McpPanel.vue:80-83` 两处；`groupTargetIds` 仍先格式化成展示文本再按 `/` 反解析）；(b) mcp 表单 parse/format/validate 三件套困在 `useMcpMutations.ts`（68-135, 241-262, 284-310）私有函数里，行级校验零直接测试——对照 providers/model.ts:397-511 + 533 行测试的已验证形态；(c) agent 清单前端残留第六份拷贝 `AVAILABLE_PROJECT_AGENTS`（`model.ts:262`，不锚定生成类型）与死导出 `BuiltinTargetPresetId`（71-78）。
- 方向：先做 (b)（照抄 providers 模板，收益/风险比最高）；(a) `parseTargetId` 单点；(c) Rust 侧 codegen 导出 AGENT_IDS 派生、死导出删除（不得手改生成文件）。
- (b) 已做：表单的解析 / 格式化 / 校验 / payload 组装收进 `workspace/mcpForm.ts`（`formFromMcp`、`buildMcpErrors`、`buildMcpPayload`、`parseKeyValueText`、`parseLineList`、`keyValueError`、`parseTimeoutText`），`model.ts` 与 `useMcpMutations.ts` 里的重复定义删除，新增 17 条行级直测（等号在值里、空值、负 timeout、按传送方式取舍字段、坏行定位、往返一致性）。`useMcpMutations` 只剩编排。

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

### 10. C11 · codex apply 写序不变量测试 · 已完成

- 域：providers；强度：Speculative（小而实的顺手项）；完成于 2026-10-12
- 落地：`codex_apply_writes_catalog_before_config` 断言应用产物里目录文件
  （`reins-models.json`）排在 `config.toml` 之前，并断言 config.toml 的
  `model_catalog_json` 确实指向那个路径。选 preview 作为观测点是因为顺序由
  adapter 返回的数组决定（`write_files` 按数组顺序逐个原子写），而 `apply` 直接
  写盘、没有可注入的写入口径——preview 是这条不变量唯一可观测的产物。
- 问题：「目录文件必须排在 config.toml 之前写入」只活在 `apps/codex.rs:324-330` 的注释里，接口类型 `Vec<(PathBuf, String)>` 对顺序零承诺。
- 方向：一个测试断言 codex apply 产物的首个文件是 reins-models.json，把注释变成被守卫的事实。

## 已完成（本轮额外做的裁决）

- **启停开关全部废弃**（`decisions/2026-10-12-deprecate-enabled-flags.md`）：target / MCP / 项目 agent 三处 `enabled` 字段删除，旧配置的键被忽略且不再写回，前端启停交互与 3 个相关测试删除。这条原本不在清单里，是实施 C1 时发现的同层分叉（同一个状态机被四条路径各解释一遍）。

## 已完成（C2 收窄后先做的三件事）

- **target 寻址收成 `TargetRef`**：复合 id 的构造与拆分只在 `workspace/types.rs` 一处（此前 3 处组串、1 处内联 `find(':')` 拆分）；`ResolvedManagerConfig::target / target_by_id / target_refs` 是唯一寻址入口，`inspect` 与分发侧不再自己拼串。
- **「target 不可用」只有一个说法**：`target_unavailable()` 供报错与 warning 共用（此前同一个故障有三种文案、两种语言），调用方只决定「报错还是记 warning」。
- **config.yaml 的 `~` 展开与清单解析同源**：统一到 `support::fs::user_home_dir()`（`dirs::home_dir()` 在 Windows 忽略 HOME，会让模板写出的路径与解析结果分叉）。

## 本轮修掉的真 bug（都不在原清单里，是实施 C2 时挖出来的）

- **project id 含 `:` 时地址不可拆**：`a:b:codex` 被拆成 project=`a` /
  agent=`b:codex` → 查询落空报「target 不存在」，但 inspection 照常展示它，
  用户看得到、点不动。写入与解析两侧此前都只 trim、无字符集校验，已收紧为禁止
  `:` 与 `/`（`803e523`）。
- **agent 名不规范时造空目录**：YAML 写 `Codex:` 时 id 规范成 `codex`，但落点
  回退用原文名拼路径 → 凭空多一个 `Codex/skills` 空目录，真正在用的
  `<project>/codex/skills` 不被识别（`21b4696`）。
- **claude 重定向把 MCP 条目写错文件**：`CLAUDE_CONFIG_DIR` 实测是完整配置 home
  （`.claude.json` / `settings.json` / `projects` / `sessions` 全部随之搬移），
  旧实现按「`.claude.json` 落在 HOME」写，用户设了变量后 reins 与 CLI 读写两个
  文件（`883fee5`）。

## 已完成：前端与后端对「整目录继承」的判定不一致

后端 `skip_reason` 只看「项目级 + skill_dir 是链接」（不看指向谁，`9fdfe82` 已把
这条规则收进寻址层并注释标明）；展示层的 `linked_target_id` 只看「是否指向另一个
受管 target」且对全局 target 也生效。由此两处不对称：

| 场景 | 前端 | 后端 |
| --- | --- | --- |
| 全局 target 的 skill_dir 指向另一受管 target | 禁用勾选、显示「继承自 X」 | 不跳过，客户端传了就照常安装 |
| 项目级 target 的 skill_dir 指向外部目录 | 不禁用、照常勾选 | 静默跳过，用户勾了没有任何提示 |

第二行是用户可见的静默失效。**已修**（`fix(workspace)` 两个提交）：

- 结论先定了语义：落点是软链接时，**写盘穿链接是正常语义、不是 bug**（仓库自己
  就依赖它，`remove_at` 的注释写明要对链接自身 canonicalize 才判归属）；要修的是
  **称呼与可选性**——谁动的、动的是谁的目录，界面必须说清。
- 契约补 `skill_dir_link_target`（链接解出的真实目录），`linked_target_id` 退回
  「命名归属」；界面据此禁用并标注「内容属谁 / 文件在哪」。
- 顺带修掉一个真 bug：`dir_to_target` 的 key 用 canonicalize 建，而别名目录
  canonicalize 后就是它的目标目录，于是别名被算成「继承自自己」。新增
  `normalize_own_dir`：归属比较只规范化父目录。
- `TargetSkip::LinkedDir` 的跳过从静默改为 warning（兜底路径不再无声）。

## 已完成：目标列表显示落点状态

- 现状（立此项时的依据）：目标面板只回显配置里的路径字符串，`TargetInspection`
  只有 `exists`，而带探测信息的 inspection 前端零消费——「落点是软链接」这件事
  只在同步弹窗可见。
- 落地：寻址层 `skill_dir_links()` 成为落点归属的单一出处（同步弹窗改为消费它，
  删掉自己那份 canonicalize 映射）；`TargetInspection.skill_dir_link` 按同一 id
  口径带给两个列表视图；`WorkspaceInspection.warnings` 从恒空 vec 变为「哪些
  target 因落点是软链接不参与分发」；前端列表按行标注「软链接 + 内容属谁 + 文件
  实际所在」，判定与文案收成纯函数 `skillDirLink.ts`。
- 提交：`8040f20`（后端）、`bb06ab0`（前端）。

## 小项（不单独立项，顺手做）

- providers 能力表 `ModelField::ReasoningLevels` 零适配器引用（`apps/mod.rs:66-79`）——2026-09-24 plan 声明的 unverified 项，验证后填充或删除。
- session 域的同类环境解析：`session/pi.rs`、`session/grokbuild.rs` 直接读 env，`session/codex.rs` / `claude_code.rs` / `dsh.rs` 各自拼默认目录（与 `support::environment` 同批事实）。C10 只覆盖 agents / workspace / providers 三处；session 的会话存储读取路径要收敛应单独立项，见 ADR「未纳入本次」。
- `CLAUDE_CONFIG_DIR` 的 **skills 落点**是目前仅剩的 `unverified`（清单按 `<CFG>/skills` 实现）：`claude skills list` 未登录不可用，畸形 skill + `--debug-file` 在 `--version` 下不产生日志。核对方式：登录后把 skill 放到 `<CFG>/skills` 观察是否被发现。
- `PathInspection.exists` 前端零渲染（`inspection.targets[].skillDir.exists`、`projects[].agents[].skillDir.exists` 都没被读）：落点目录不存在这件事界面不说。补提示属产品决定（可能是空目录待安装，也可能是配置写错），先记着不动。
- `model.ts:72` 与 `workspace/styles.css:72` 残留 `--color-text-muted` / `--color-muted` 两个令牌，`src/styles.css` 里只定义了 `--color-muted`；工作区那份引用不存在的令牌（浏览器静默忽略）。顺手清掉或补定义。

## 已完成（2026-10-11 ~ 10-12，作为上下文保留）

- C1 安装面深模块（`decisions/2026-10-12-skill-install-surface.md`）：目标 skip 判定、落点计算、链接三态分类、按来源卸载四条路径收进 `workspace/skill_install.rs`；`DestinationState` 取代原先按调用方分叉的 4 种判定；单 target 最小夹具 5 个直接测试。行为逐条对齐（含一条隐性前置于写入的「创建父目录」）。
- C10 全项（`decisions/2026-10-12-global-root-env-redirect.md`）：重定向知识收进 `support::environment`（进程环境唯一读取点，`Redirect` 同持变量名与默认目录），`GlobalRoot` 认 CODEX_HOME / CLAUDE_CONFIG_DIR / DSH_HOME 且与模板渲染共用判据；providers 路径函数从清单派生、原 `ToolEnv` 删除；workspace 环境挂在 store 上；`tests/providers.rs` 的第三份路径助手删除。回归测试 `tests/workspace.rs::codex_mcp_apply_follows_codex_home_redirect` 先失败后通过；两个 claude 语义点仍标 `unverified`。
- C8 方言表（`4994b5d` / `c31b33b`）：归一化枚举拼写单一来源 + roundtrip 守卫；修复 dsh `off→none` 映射文案活 bug。
- Windows 测试基线（`ea152bb` / `8371e1c`）：修复 claude delete plan 漏 meta.json、grokbuild delete plan 漏子会话目录两个真 bug；`GlobalRoot::resolve` 走 `user_home_dir`；快照比较统一分隔符口径。
- C13 token 展示口径（`6f088cb`）：收敛到 `sessions/token-usage.ts` 单一来源。
- C5 SQL 来源部件（`68b2961` / `b68ac69`）：父链分组（孤儿策略显式化）、用量口径双入口、tool 块装配收编；dsh 不入父链部件、孪生 usage 测试不强行收敛，见 `decisions/2026-10-12-sql-source-parts.md`。
- C12 分页状态机（`11a3596`）：`createPagedList` + `infinite-scroll` 助手，四个调用方塌缩为适配器，新增 14 个状态机直接测试。
- 范围裁决：子代理弹窗 mini-loader 是单次拉取形态、非分页，不收编。
