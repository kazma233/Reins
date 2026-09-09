# skill discovery preview 收敛为共享模块

- 日期：2026-08-19
- 状态：生效中

## 背景

`useSkillImport` 与 `useSkillSourceEdit` 两个 composable 逐字符共享约 140 行：latest-wins 竞态守卫全家桶、discover→filter 流水线、300ms debounce watch、`applyDiscoveryResult` 模式守卫。这不是抽象不足，是同一个模块没被认出来命名。此外全项目手写守卫散布（sessions 域 3 处 key-based 守卫）。

## 决策

1. **`shared/lib/request-guard.ts`**：latest-wins token 工具（`next()/isLatest()/invalidate()`），无业务语义，放 shared/lib 符合域边界红线。
2. **`useSkillPreview(host, refreshErrorText)`**：discovery preview 控制器，持有竞态守卫、discover→filter 流水线、debounce 模式重过滤，绑定宿主 dialog 的 preview 状态。import 与 source-edit 两个 composable 只留各自的 dialog 编排。
3. **host 契约**：`{ open: boolean; preview: SkillDiscoveryPreviewState }` 结构类型。host 必须是 reactive dialog 对象——`Object.assign` 重置 state 会替换 `preview` 属性引用，工厂内部每次经 host 重新读取，不缓存 preview 引用。
4. **模式文本守卫保留**：filter 响应回来时比对 `includeNamePatternsText` 与请求时快照（`applyDiscoveryResult` 的 expected text 参数），防旧 filter 结果覆盖用户已输入的新 patterns。这是 dialog 稳定性（2026-06-12 import-preview-dialog-stability）的核心约束，随引擎一并集中。
5. **死出口删除**：`handleSourceTypeChange` / `handleImportModeChange` 无消费者（dialog 现在直接赋值 state），连同 `import_workspace_skill` 别名命令（纯转发 `import_discovered_skills`）一并清除。

## 已知限制

- sessions 域 3 处 key-based 守卫（`detailKeyRef` / `sessionListRequestKeyRef` / `activeDetailKeyRef`）是 key 比较语义（token 是自增数的特例），随候选7 治理 SessionDetail.vue 时再统一，本批不动 sessions 域。
- 测试环境无 DOM，`setTimeout` 用全局而非 `window.` 前缀。

## 排障记录（过程性，见 context/）

- 曾发现 `src/` 下 69 个 `.js` 编译泄漏物（一次误跑的无 noEmit tsc 产物）污染 vite/vitest 模块解析（`.js` 优先于 `.ts`），导致测试 import 到旧代码。已全部清除；这类问题的特征是「改了 .ts 但测试行为不变」。

## 验证

- vitest 15 通过（新增 useSkillPreview 6 个：latest-wins、invalidate 丢弃、模式文本守卫、debounce 合并单次调用、模式已匹配跳过 filter、无模式直通）。
- `vue-tsc --noEmit` 零错误、`pnpm build` 成功、`cargo test` 112 通过（Rust 侧仅删别名命令）。
- useSkillImport 299→129 行、useSkillSourceEdit 300→150 行，SkillsPanel.vue 零改动。
