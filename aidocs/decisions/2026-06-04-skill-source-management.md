# Skills 导入项管理对话框

- 日期：2026-06-04
- 状态：生效中

## 背景

Skills tab 之前只有一个 `同步导入项` 按钮：点开后一次性同步所有 source，期间没有按 source 控制的入口；编辑 source 的入口散落在导入流程；删除 source **完全没有 UI 也没有后端 command**。同时 `delete_workspace_skill_inner` 在软链接 project target 上直接 `remove_existing_path`，会破坏软链接本身，是个潜在 bug。

## 决策

把 `同步导入项` 按钮替换为 `管理导入项`，打开一个新的 `SkillSourceManageDialog`，并新增后端 `delete_skill_source` / `delete_skill_sources` 两个 command。整体行为：

- 列表：每个 source 一行，复用 `manager-workspace-row` 视觉语言（行 + 主区 + 垂直操作按钮）。
- 单行操作：`编辑` / `同步` / `删除` 三个按钮垂直堆叠。
  - `编辑` 复用现有 `SkillSourceEditDialog`，但为了让同一个组件能在管理对话框之上叠出，把 `handleConfirmSkillSourceEdit` / `handleRefreshSkillSourceEditPreview` 改成接受 `SkillSourceEditDialogState` 参数（不再闭包全局 state）。
  - `同步` 走单 source 的 `syncSkillSource`，不需要 cover UI，只用 toast 提示，并 union 写入 `skillSyncState.addedSkillIds` / `missingSkillIds` 以保持 `sourceFilter === "新增"` 过滤生效。
  - `删除` 走二次 `ConfirmDialog`，确认后调 `delete_skill_source`。
- 顶部工具按钮：`批量同步` / `批量删除`。
  - `批量同步`：勾选 source → 点 `批量同步` → z-index 200 cover（spinner + `正在同步 N 个导入项...` + `取消同步` 按钮）→ 循环调 `syncSkillSource` 并累计 union → 同步结束 → z-index 300 结果对话框（每个 source 一行 `success-pill` / `danger-pill`）→ 用户点 `确定` 关闭。期间管理对话框始终打开。
  - `批量删除`：勾选 source → 点 `批量删除` → 二次 `ConfirmDialog` → `delete_skill_sources`。
- 取消语义：cover 的 `取消同步` 调 `AbortController.abort()`，循环检测 `signal.aborted`，已发出的单个 sync 仍会跑完（Rust 端没 cancellation token）。这是用户能接受的最强语义。

## 软链 project target 跳过 → 全面简化为「只清 yaml + workspace 副本」

**最早的设计**是抽出私有 helper `purge_skill_from_targets(resolved, workspace_path, skill_name) -> Result<Vec<String>>`，对每个 target 检查 `is_project && is_linked_skill_dir(&skill_dir)?`，是则跳过；否则 `remove_existing_path(target.skill_dir.join(skill_name))` 并记录。

该 helper 同时被 `delete_workspace_skill_inner` 与 `delete_skill_source_inner` 使用——**顺带修复了** `delete_workspace_skill_inner` 在软链 project target 上误删软链接的潜在 bug。新增 `delete_workspace_skill_now_skips_linked_project_targets` 单测作为回归保护。

**最终决策（2026-06-04 修订）**：删 source **不**清理 target，理由：

- `delete_skill_source_inner` 只清 nyleen.yaml 中该 source 条目、旗下 skills、`<workspace>/skills/source/<name>` 副本
- target 内的链接/副本由用户自行管理——这是更直观的语义（target 是用户已分发的产物，不应被 source 删除"间接清掉"）
- 软链 project target 的特殊情况也随之失效（不再需要 `is_linked_skill_dir` 守卫）

helper `purge_skill_from_targets` 删除；`delete_workspace_skill_inner` 恢复内联循环（保留原行为，不修"软链 project target 上 `remove_existing_path`"的历史问题）。

## 当前边界

- `SkillSourceEditDialog` 组件本身没改，只调整 `workspace.tsx` 里两个 handler 的签名（接受 state 参数）。
- 单 source `同步` 不展示 cover，与批量同步的体验刻意区分。
- 取消批量同步时已发起的 `syncSkillSource` 命令无法从前端 abort，只能停止发起新的请求。
- 结果 dialog 只有 `确定` 一个出口，不允许"重试"。

## 前后端 Contract 变更

| 前端位置 | 后端位置 | command / DTO | 说明 |
| --- | --- | --- | --- |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `delete_skill_source` | 删除单个 source，返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `delete_skill_sources` | 批量删除 source（顺序循环，前一个失败 bail） |

## UI 组件清单

- `src/features/workspace/dialogs.tsx`：
  - 新增 `SkillSourceManageDialog`（管理弹窗主体）
  - 新增 `SkillSourceSyncCover`（批量同步 cover，z-index 200）
  - 新增 `SkillSourceSyncResultDialog`（结果 dialog，z-index 300）
  - 删除 `SkillSyncDialog` 及死代码 helpers
- `src/shared/ui/dialog-shell.tsx`：新增 `zIndex` prop
- `src/features/workspace/model.ts`：新增 `SkillSourceManageDialogState` / `SkillSourceManageOverlay` / `SkillSourceBatchSyncItemResult` / `SkillSourceBatchSyncResult`；删除 `SkillSyncDialogState` / `SkillSyncDialogPhase` / `createSkillSyncDialogState`

## 行为不变项

- `sync_skill_source` / `update_skill_source` 命令不变
- 8 处 `resetSkillSyncState` 调用点继续工作；`skillSyncState.addedSkillIds` 仍由新批量同步流程以 union 形式写入

## 2026-06-04 原则变更：manager 不主动删除

**新增原则**：manager 工具只负责写入（同步 / 导入 / 编辑），不负责主动删除。

- **写入**：
  - 全局 target（`is_project=false`）→ 软链接到 `workspace/skills/source/<name>` 副本
  - project target（`is_project=true`）→ 真 copy 一份
- **删除**：
  - `delete_skill_source`（管理弹窗的「删除」/「批量删除」）→ **只**清 nyleen.yaml + 我们自己空间下的 `workspace/skills/source/<name>` 副本
  - 各 target 下已分发的内容（全局 target 的软链 / project target 的 copy）→ **由用户自行管理**
  - exclude patterns 变化导致 sync 时认为 skill 不再属于该 source 也不会从 target 撤回

**移除的功能**：
- 后端 `delete_workspace_skill` / `delete_workspace_skills` 命令、`delete_workspace_skill_inner` / `delete_workspace_skills_inner` inner 函数
- 前端 `api.deleteWorkspaceSkill` / `deleteWorkspaceSkills` wrapper
- 前端 `Workspace` 里的 `skillDeleteDialog` state、`openSkillDeleteDialog` / `closeSkillDeleteDialog` / `handleDeleteWorkspaceSkill(s)` / `handleConfirmDeleteSkills` handler 与 stable callback
- `SkillsPanel` 的批量多选（`selectedSkillIds` / `toggleSkillSelection`）+ SkillCard 的 [删除] 按钮 + 「missing」pill
- helper `purge_skill_from_targets`（原本为「软链 project target 跳过」准备，已无需）

**保留**：`delete_skill_source` / `delete_skill_sources` 仍在，作为「我们内部状态清理」操作。

## 代码落点

- `src-tauri/src/workspace/mod.rs` (`delete_skill_source_inner`, `delete_skill_sources_inner`, 1 个 test `delete_skill_source_removes_workspace_copies_and_yaml_entries`)
- `src-tauri/src/workspace/commands.rs` (`delete_skill_source`, `delete_skill_sources`)
- `src-tauri/src/lib.rs` (注册 command)
- `src/shared/ui/dialog-shell.tsx` (`zIndex` prop)
- `src/features/workspace/model.ts` (新类型 / factory)
- `src/features/workspace/dialogs.tsx` (`SkillSourceManageDialog`, `SkillSourceSyncCover`, `SkillSourceSyncResultDialog`, 删除 `SkillSyncDialog`)
- `src/features/workspace/api.ts` (新 wrapper)
- `src/features/workspace/workspace.tsx` (state、handlers、render 替换)
- `src/features/workspace/panels.tsx` (按钮文案 / prop 重命名)
- `src/features/workspace/styles.css` (新 class)
- `README.md`
- `aidocs/decisions/2026-05-22-frontend-backend-contracts.md`
