# Dialog 归属下沉到业务面板

- 日期：2026-08-18
- 状态：生效中

## 背景

面板自治重构前，`Workspace.vue` 作为集中编排层存在几个问题：

1. **16 个 dialog 集中渲染**：skills / mcp / targets / projects 四个域的弹窗全部挂在 `Workspace.vue` 模板尾部，文件膨胀到 568 行，改任何一个域的弹窗都要动壳组件。
2. **emit 转发长链**：面板不持有 mutation 状态，只能 `emit` 事件 → `Workspace.vue` 接住 → 调 composable → composable 改 dialog state → dialog 又渲染在 `Workspace.vue`。一个"删除 source"要跨三层文件。
3. **ConfirmDialog 零使用**：8 个内联确认框（删除 source / mcp / target / project、覆盖同步等）各自手写 `DialogShell` + 按钮模板，结构雷同。
4. **状态放错位置**：mutation composable（如 `useSkillImport`）里的 dialog state 只允许被调用一次，却被放在 `Workspace.vue` 顶层，面板无法自治。

## 决策

1. **dialog 状态与渲染归属各业务面板**：`SkillsPanel` / `McpPanel` / `TargetsPanel` 各自调用域内 composable、各自渲染域内 dialog，面板无 props/emits，数据自取（store / `useWorkspaceState`）。
2. **确认类弹窗统一用 shared `ConfirmDialog`**：不再手写 `DialogShell` + actions 模板，8 个内联确认框全部收敛。
3. **跨域的 project agent picker 拆为 `useProjectAgentPicker`**：由 `McpPanel` 消费，`useProjectMutations` 瘦身回 target/project 域自身。
4. **`Workspace.vue` 只做壳**：header 文案 + tab 导航（`store.setWorkspaceTab`）+ 三个 Panel + `AppToast`（notice 来自 `useWorkspaceNotice` 单例）+ `onMounted` 单一 bootstrap（`reloadWorkspaceState`）。568 行 → 58 行。

## 生效约束

- 新增 mutation / dialog 时放进对应面板域，不再上提到 `Workspace.vue`。
- `DialogShell` 通过 Teleport 挂到 body，dialog 放在任意组件内都不受父级 `overflow` / 层级影响，位置自由。
- store / notice 是单例（Pinia store + 模块级 ref），composable 可被多个面板安全调用，不会产生多份状态。

## 验证

- `pnpm exec vue-tsc --noEmit` 全项目零错误
- `pnpm build` 完整通过
- Grep 确认无文件引用 `dialogs/BatchGitImportResultList`、`dialogs/SkillDiscoveryPreview`、`dialogs/SyncTargetGroups` 旧路径
