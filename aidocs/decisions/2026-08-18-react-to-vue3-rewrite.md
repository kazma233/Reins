# 前端从 React 19 重写为 Vue 3

- 日期：2026-08-18
- 状态：生效中

## 背景

原前端基于 React 19 + Zustand 5 + Vite 8，后端是 Tauri + Rust。React 版本的几个长期痛点：

1. **胖组件**：`workspace.tsx` 2202 行、`dialogs.tsx` 1767 行、`session-detail.tsx` 1500+ 行，单文件承载所有业务逻辑。
2. **性能优化的反模式**：为配合 `React.memo` 大量使用 `useCallback` / `useStableCallback` / `useShallow` / `useDeferredValue`，代码噪音大，且容易因依赖数组写错导致 stale closure。
3. **受控表单冗长**：每个表单字段都要写 `value` + `onChange` handler，表单密集的 workspace 对话框代码量膨胀。

后端 31 个 Tauri invoke 命令稳定，前端重写不影响 Rust 侧。

## 决策

### 1. 技术栈

- Vue 3.5 `<script setup>` + Pinia 2 + VueUse 11
- Vite 8 + `@vitejs/plugin-vue`（构建链不变）
- `vue-tsc` 替代 `tsc` 做 SFC 类型检查
- 不引入路由库（原本就是 tab 切换，无路由）
- CSS 沿用现有 BEM 全局类名，不引入 CSS Modules / scoped

### 2. 目录结构

保持域边界不变：
- `app/`：顶部模式切换 + 全局 store
- `features/sessions/`：历史会话域
- `features/workspace/`：配置与分发域
- `shared/`：UI 组件 + lib 工具（不承载业务语义）

新增 `composables/` 目录替代 React 的 `hooks/`，按业务流程拆分胖组件。

### 3. React → Vue 模式映射（关键，避免照搬 React）

| React | Vue 3 |
|-------|-------|
| `useState` | `ref` / `reactive` |
| `useEffect([dep])` | `watch(dep, cb)` |
| `useEffect([])` mount | `onMounted` |
| `useMemo` | `computed` |
| `useCallback` / `useStableCallback` | 普通函数（setup 只执行一次，闭包天然稳定） |
| `memo(Component)` | 不需要（Vue 按字段订阅） |
| `useShallow` | `storeToRefs`（Pinia 天然按字段订阅） |
| `useDeferredValue` | VueUse `refDebounced` |
| `useRef` 跟踪 id 防竞态 | `ref` 保留（仍需要） |
| 受控 `value` + `onChange` | `v-model` / `defineModel` |
| `createPortal` | `<Teleport to="body">` |
| `children` prop | `<slot>` |
| `className` | `class` |

### 4. 拆胖组件

React 版 `workspace.tsx` 2202 行的所有业务编排拆成 6 个 composable：
- `useWorkspaceState`：初始化 + 全量刷新
- `useSkillImport`：skill 导入流程
- `useSourceSync`：source 同步 + 移除同步
- `useSkillSourceEdit`：source 编辑 + 删除
- `useMcpMutations`：mcp 增删改 + 应用到 target
- `useTargetMutations` / `useProjectMutations`：target / project 增删改

`session-detail.tsx` 1500 行拆成 `SessionDetail` + `MessageTimeline` + `MessageCard` + `EventTimeline` 四个组件 + `session-detail-helpers.ts` 纯函数集合（DRY，原 React 版在两个文件里重复定义了 `shellQuote`、`codexResumeCommand` 等）。

### 5. 直接复用的文件

纯 TS、无 React 依赖的文件直接复用，不重写：
- `features/sessions/api.ts`、`types.ts`、`source-app.ts`
- `features/workspace/api.ts`、`types.ts`、`model.ts`
- `shared/lib/*`
- 所有 `.css` 文件

### 6. 保留的关键设计

- **`SourceSyncDialog` 自管 state**：其他对话框都受控，唯独它自己加载 + 维护选择状态。Vue 版保持这个模式。
- **Skill preview 的 debounce + 竞态防护**：`previewRequestId` ref 防止旧请求覆盖新结果。
- **`persistWorkspaceMcp` 的串行 await**：编辑 mcp 时遍历 targetIds 串行 apply/remove，**不并发**（会写冲突）。
- **`reloadWorkspaceState` 全量刷新**：所有 mutation 后拉全量 `getWorkspaceState`，不做增量更新。

## 影响

- 前端代码量显著下降（表单代码因 `v-model` 大幅简化；`useCallback` / `memo` / `useShallow` 噪音全部移除）。
- 后端 `src-tauri/` 零改动，Tauri invoke 命令契约不变。
- CSS 全部复用，视觉无变化。
- 类型检查工具从 `tsc` 换成 `vue-tsc`，`pnpm check` 脚本同步更新。

## 验证

- `pnpm vue-tsc --noEmit` 零错误
- `pnpm dev` 启动成功
- 浏览器渲染正常（顶部导航 + tab 切换 + 内容区可见）
- 后端测试 `cargo test` 38 passed 不受影响
