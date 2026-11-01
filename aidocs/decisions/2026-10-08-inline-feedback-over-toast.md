# 反馈就地化：移除全局 toast

- 日期：2026-10-08
- 状态：生效中

## 背景

反馈此前统一走左下角 toast（`@shared/ui/AppToast.vue` + `notice` 单例，3.2s 自动消失）。四个问题：

1. **位置与信息无关**：表单校验错误属于某个输入框，却弹在窗口左下角；用户要先看提示、再回去找字段。
2. **生命周期错**：加载失败、同步结果都是需要用户处理或复核的信息，3.2s 后消失，用户回头看不到失败原因，也没有重试入口。
3. **成功提示是噪音**：写操作成功后列表 / 卡片 / 弹窗本身已经更新，成功句不承载新信息。
4. **三个 host**：`Workspace.vue` / `ProvidersWorkspace.vue` / `SessionWorkspace.vue` 各自渲染 toast，`SessionWorkspace` 还本地复制了一份 notice 实现，改动成本与漏改风险都在。

## 决策

反馈按语义分成四类，全部就地表达，不再有全局浮层：

| 语义 | 表达方式 | 组件 / 落点 |
|---|---|---|
| 表单字段校验失败 | 字段下方常驻红字，字段变化即清空 | `@shared/ui/AppFieldError.vue` |
| 区域加载 / 操作失败 | 出错区域原位的错误条 + 重试按钮 | `@shared/ui/AppLoadError.vue` |
| 写操作成功 | 不提示，界面自身状态即反馈（列表更新、弹窗关闭、开关翻转、错误条消失） | — |
| 需要复核的运行结果（同步数量、部分失败目标、跳过原因） | 就地常驻在触发它的卡片 / 面板 / 弹窗按钮旁，直到下一次同类操作 | 卡片警告、面板顶部结果、同步弹窗「开始同步」按钮旁的结果句 |

配套约定：

- **校验一次算全部**：提交时一次算出所有字段错误并全部展示，不逐条弹；用户能看到所有待改字段，而不是修一个才发现下一个。
- **操作失败留在发起处**：弹窗内的操作失败时弹窗不关，错误写在弹窗里可重试或取消；卡片级刷新失败写在卡片上。
- **结果错误按作用域选槽位**：整页读取失败 → store 的 `error`（面板顶部错误条）；单卡片刷新失败 → store 的 `appErrors`（卡片内）；面板直发操作失败 → store 的 `actionResults`（面板顶部）；局部预览、同步弹窗结果 → 组件内部状态，原位展示。
- **弹窗内的操作不因成功而关窗**：结果留在动作按钮旁（如同步弹窗底部「开始同步」左侧），用户能接着操作；只有要求「确认后回到列表」的写操作才关窗。
- **操作行两端的固定位置不被结果破坏**：结果文案在左、动作按钮组在右。结果用 `margin-left: auto` 把「结果 + 按钮」顶到右侧，结果自己限宽（`max-width`）并内部换行/滚动，按钮靠 `flex: 0 0 auto` 贴右不变；操作行 `flex-wrap: nowrap`，结果再长也不会把按钮挤到下一行。
- **按钮旁的结果不能让按钮位移**：结果出现 / 消失 / 换行都不能改变按钮位置。做法是在操作行固定预留左侧结果位，结果绝对定位在其中（见 `.manager-sync-dialog .dialog-actions` 与 `.manager-sync-result`），而不是让它参与流布局。
- **重试幂等且常驻**：错误条在重试开始或下一次成功时清空，不自动重试，不做退避轮询。
- 自动加载类交互（会话列表滚到底自动加载更多）在错误未清除前必须停止自动重试，避免失败循环。

## 移除的东西

- `src/shared/ui/AppToast.vue`、`src/shared/ui/app-toast.css`
- `src/shared/lib/notice.ts`、`src/features/workspace/composables/useWorkspaceNotice.ts`、`src/features/providers/composables/useProvidersNotice.ts`
- 样式令牌 `--z-toast`
- action 包装层的 notice 语义：`runWorkspaceAction` / `runProvidersAction` 的 `success` / `error` 不再负责展示，改为 `reload` + `onSuccess` / `onError`，由调用方决定落点

## 生效约束

- 新增反馈时先判断语义属于上表哪一类，不要引入新的全局浮层。
- 成功路径默认不加提示；只有当结果无法从界面状态看出（数量、部分失败、被跳过的目标）时才就地留一句常驻结果。
- 字段错误与字段一一对应并清空时机明确（用户改动该字段时）；不要保留跨操作不清空的错误状态。
- 字段错误与输入框用 `id` + `aria-describedby` 成对关联（仅在有错误时绑定），错误文本自带 `role="alert"`；不要只把错误文字放在字段旁边。
- 错误文案要写清「发生了什么 + 当前可做什么」，不要只写「失败」。

## 验证

- `pnpm exec vue-tsc --noEmit`、`pnpm build` 通过
- `pnpm vitest run` 全绿（`useWorkspaceAction` / `useProvidersAction` / `useTargetMutations` / `useSkillPreview` 断言新契约：`onSuccess` 在 reload 之后、`onError` 收到 `extractErrorMessage` 结果、字段错误状态）
- 全仓 grep `AppToast|showNotice|clearNotice|preserveNotice|z-toast` 无残留
