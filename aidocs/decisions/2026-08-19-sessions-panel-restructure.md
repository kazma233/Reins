# sessions 前端对齐 workspace 结构（候选7）

- 日期：2026-08-19
- 状态：生效中（2026-09-22 起 `useSessionDetailActions` 只保留删除流，导入相关描述仅作历史记录）

## 背景

sessions 域仍是 Vue3 重写前的旧形态，恰好是 2026-08-18 ADR 描述并已在 workspace 治愈的三种病：
① SessionDetail 胖组件（691 行）同时持有三条 async 流、13 个复位 ref、导入/删除动作与竞态守卫；
② SessionDetailDialogs 的 `formatAppName` 从 .vue 文件 export（非 setup script），copy 常量内嵌组件；
③ 手写 key-based 守卫散布三处（SessionWorkspace / useSessionTimeline / SessionDetail 双 ref 快照）。
另：ProjectAgentPicker 的 `"skill"` context 分支是从未接线的半成品。

## 决策

1. **`useSessionDetailActions(overviewRef, detailKey, onDeleted)`**：导入/删除流全部状态与动作（preview/import/delete + dialog open + reset watch + preview 重载 watch）移入 composable。SessionDetail.vue 回到「过滤派生 + 模板 + 无限加载」，691 → 513 行。
2. **`sessions/model.ts`**（对齐 workspace/model.ts）：IMPORT/DELETE_METHOD_COPY、translateWarning、formatImportLevel、resume 命令族、deleteCommandPreview 从 helpers 与 dialogs 移入。helpers 只剩 timeline 派生纯函数（497 → 402 行）。`formatAppName` 的 .vue export 消除，直接用 source-app 的 `formatSourceAppName`。
3. **删除确认换 ConfirmDialog**：delete dialog 的手写 DialogShell + actions 模板收敛，富内容走 default slot；import/importResult 保持 DialogShell（非确认类）。
4. **守卫统一到 request-guard**：新增 `createKeyGuard<T>(read)`（key 比较变体，覆盖「失效信号是外部值变化」的场景）。替换三处：useSessionTimeline 的 `detailKeyRef` 镜像（ref+watch 别名化，等价直读）、SessionWorkspace 的 `detailRequestVersionRef`（纯自增 token，用 `createRequestGuard`）、SessionDetail 的双 ref 快照（组合 key `${detailKey}::${targetApp}`）。SessionWorkspace 的 `sessionListRequestKeyRef` 保持（复合字符串 key，已是最简形态）。
5. **ProjectAgentPicker skill 死分支删除**：`"skill"` context 类型、`skillId` 死字段、dialog 内 4 处分支、恒空 `linkedAgentIds` 钩子链（isLinkedSelected/isAgentLinked/linked tone）一并清除。picker 现为纯 MCP 域组件；未来 skill 接入时按新需求重新设计，不保留空壳。

## 已知限制

- key guard 是字符串/值相等比较：detailKey A→B→A 往返时旧响应可通过（原实现同样存在，非回归）。
- translateWarning / deleteCommandPreview 仍是影子契约（Rust 整句警告原文做 key、前端复述删除 SQL），恢复手段是后端出稳定 code + 命令清单（contracts ADR 第③步），本批只做归位不动行为。

## 验证

- `vue-tsc --noEmit` 零错误、vitest 15 通过、`pnpm build` 成功（sessions 域无新增测试面：actions composable 的竞态语义与 useSkillPreview 同构，其守卫由 request-guard 单元覆盖）。
- 行数：SessionDetail 691→513、SessionDetailDialogs 405→250、helpers 497→402；新增 model.ts 247、useSessionDetailActions.ts 237。净 -520 行。
- 全仓净 diff：10 文件 -624/+104（不含新增文件）。
