# 会话身份显式化：接口只认（来源, id）

- 日期：2026-10-09
- 状态：生效中
- 前置：`2026-10-08-reader-engine.md`（读取器引擎）；`2026-10-08-session-source-registry.md`（来源注册表）

## 背景

架构评审候选 5：`transcript_path` 此前身兼三职——展示字段（详情页路径）、前端主键（stores 去重/选中 key）、后端寻址凭据（SessionReader 五个 parse 方法与全部 detail/delete 命令的入参）。接口声称「path 唯一标识一个会话」，但 opencode/zcode 没有文件、用 `"db路径:id"` 合成串伪装 Path 穿过整个索引/缓存键体系（zcode 在 db 解析失败时静默回退 `/tmp/zcode.db` 以维持「总能造出 Path」）；timeline.rs 还因此出现「把前端传来的 path 当不可信输入重新校验归属」的 GrokBuild 防御特例——身份系统缺位的补丁。

## 决策

1. **`SessionReader` 五个 parse 方法入参从 `&Path` 改为 `source_session_id`**；`resolve_path` 从 trait 退役，id→path 解析成为各 reader 私有实现（pi 保留原有逐文件 header 扫描为私有 `resolve_session_path`）。会话身份 = `(SourceApp, source_session_id)` 二元组，`transcript_path` 只作展示字段。
2. **family_index 双轨收敛为单一 id→family 直查**：`sessions_by_path`（path→family）与 `FamilyIndex::build_with_ids`（可选 id 图）退役，只留一种 build——全部来源统一建 id→family 下标图；`FamilySpec` 的 `id_map()`/`resolve_path()` 钩子及 opencode/zcode 的派生覆写随之删除（「不需要 id 图」的理由本就是它们用假路径寻址）。
3. **三条胜者规则由前置核验测试先钉住再改造**（改造前 9/9 绿）：同族成员 id 重复时后插入者胜（两条同 id 成员必同族，归属无歧义）；跨族成员 id 碰撞时排序靠后的族胜；跨族 family key 碰撞时首见占位（较新的族胜）。id 直查逐字保持同规则。
4. **命令参数收窄**：overview/messages/events/agent_messages/delete_session/get_delete_plan 全部去掉 `transcript_path` 参数；`DeletePolicy::Deleter.delete` 改 `fn(&str)`。GrokBuild 的 path 归属重校验防御删除（id 解析后端新鲜进行，错配失去存在理由）。
5. **前端主键改 `${app}:${sessionId}` 复合键**；api.ts 全部 detail 类 invoke 从三元组砍成二元组；`transcript_path` 渲染用途不变。DTO（ts-rs 生成物）零变化。
6. pi 条目（summary=None 的唯一来源）在 `SessionFileEntry` 补内部字段 `source_session_id`（非 DTO）供列表层摘要回退寻址——header 本就读，只是不再丢弃。

## 生效约束

- 跨层寻址会话只用 `(来源, id)`；任何新命令/前端 key 不得再引入 path 寻址或 path 主键。
- opencode/zcode 的 `"db:id"` 合成串仅存于 `FamilyRow::member_path` 内部身份与 `transcript_path` 展示出处，不参与对外接口。
- id 碰撞语义以 family_index 的胜者规则测试为准，改动需同步更新该测试并在此文档记录。

## 验证

- 前置核验：3 条胜者规则 9 用例对改造前代码全绿后才动工。
- 既有测试机械适配约 160 处（断言内容保持；三类内在变化：path 形态错误文案→id 形态、成员路径断言→家族归属断言、GrokBuild 外部路径攻击面用例→未知 id 用例）。
- `cargo fmt --check` / `cargo check --all-targets` 0/0；`cargo test` 374 通过 / 0 失败 / 2 忽略；`pnpm test` 168 通过；`pnpm build` 通过；generated 零 diff。
- 净 -151 行（38 文件，+575/−727）；`support::fs::find_session_file` 随默认 resolve 钩子退役而删除。
