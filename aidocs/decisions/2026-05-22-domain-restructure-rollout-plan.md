# 按业务域重组仓库的拆分落地方案

- 日期：2026-05-22
- 状态：执行中

## 背景

当前仓库已经稳定存在两大业务域：

1. `sessions`：历史会话发现、浏览、详情、导入、删除。
2. `workspace`：`nyleen.yaml` workspace 管理，以及 targets、skills、mcps 的配置与分发。

现有目录没有完全围绕这两个业务域组织，主要问题是：

1. 前端以 `components / features / hooks / lib / styles` 的技术分层为主，同一业务被拆散在多个目录。
2. `manager` 不是业务概念，只是页面视角，却承载了整个 workspace 域。
3. `src/lib/` 和 `src-tauri/src/shared.rs` 都已经演变成“混合层”，共享代码和业务代码混在一起。
4. 前后端都存在命名不准和归属错误，例如 app 级状态放在 manager store、`SessionDetail` 实际上只是 overview、`DialogShell` 依赖 `session-detail.css` 提供共享样式。
5. Rust 端 `manager.rs`、`app.rs`、`shared.rs`、`state.rs` 体量和职责都不合理，已经影响后续扩展。

## 目标

1. 目录按业务域收口，前端围绕 `sessions` 和 `workspace` 组织，后端围绕 `session` 和 `workspace` 组织。
2. `shared` 只保留真正共享的 UI、基础工具和稳定支持模块，不再承载业务逻辑。
3. 逐步移除 `manager` 作为核心命名，代码层统一收敛到 `workspace`。
4. 每个阶段都可以单独落地、单独验收，不依赖一次性大重构。
5. 在重组过程中尽量不改行为，不把“移动目录”和“功能重写”混成一个变更。

## 非目标

1. 不在这轮拆分里重做 UI 交互。
2. 不更换 React、Zustand、Tauri、Rust 技术栈。
3. 不为了目录重组引入长期兼容层。
4. 不在第一轮就把所有大文件深拆成最细粒度；先完成归位，再处理深拆。
5. 不修改现有存储格式、会话文件格式、`nyleen.yaml` 结构，除非拆分过程中发现明确 bug。

## 最终命名定稿

| 范围 | 当前 | 最终 |
| --- | --- | --- |
| app mode | `"sessions" \| "manager"` | `"sessions" \| "workspace"` |
| 前端域目录 | `features/session-*`、`features/manager-*` | `features/sessions/`、`features/workspace/` |
| 前端通用层 | `components/`、`lib/` 混放 | `shared/ui/`、`shared/lib/` |
| workspace 相关类型 | `Manager*` | `Workspace*` |
| workspace tab | `ManagerTab` | `WorkspaceTab` |
| 前端 overview 类型 | `SessionDetail` | `SessionOverview` |
| Rust overview 类型 | `SessionDetailOverview` | `SessionOverview` |
| Rust workspace 根模块 | `manager.rs` | `workspace/` |
| Rust session 命令入口 | `app.rs` | `session/commands.rs` 或 `session/mod.rs` 门面 |

补充约束：

1. UI 文案继续保留“历史会话”和“配置与分发”，不强行把用户可见文案改成 `workspace`。
2. `SourceApp` 继续留在 session 域，不升格为全局共享概念。
3. `BuiltinTargetPresetId`、`AgentTargetId`、`McpConfigType` 继续留在 workspace 域，不和 session 域混用。

## 最终目标目录

### 前端

```text
src/
  App.tsx
  main.tsx
  app/
    store.ts
    components/
      top-mode-switch.tsx
      top-mode-switch.css
    styles/
      shell.css
  features/
    sessions/
      workspace.tsx
      api.ts
      store.ts
      types.ts
      source-app.ts
      hooks/
        use-session-timeline.ts
      components/
        session-list.tsx
        session-list.css
        session-detail.tsx
        session-detail.css
        session-detail-dialogs.tsx
        source-switcher.tsx
        source-switcher.css
      styles/
        workspace.css
    workspace/
      workspace.tsx
      panels.tsx
      dialogs.tsx
      api.ts
      store.ts
      types.ts
      model.ts
      styles.css
  shared/
    ui/
      app-select.tsx
      app-select.css
      app-toast.tsx
      app-toast.css
      confirm-dialog.tsx
      confirm-dialog.css
      dialog-shell.tsx
      dialog-shell.css
    lib/
      errors.ts
      format.ts
      join-classes.ts
  vite-env.d.ts
```

### Rust / Tauri

```text
src-tauri/src/
  lib.rs
  main.rs
  logger.rs
  session/
    mod.rs
    commands.rs
    model.rs
    catalog.rs
    timeline.rs
    import.rs
    delete.rs
    text.rs
    jsonl.rs
    codex.rs
    claude_code.rs
    opencode.rs
  workspace/
    mod.rs
    commands.rs
    types.rs
    preferences.rs
    document.rs
    inspect.rs
    targets.rs
    skills.rs
    mcps.rs
  state/
    mod.rs
    session_index.rs
    skill_discovery.rs
  support/
    fs.rs
    paging.rs
    time.rs
  tests/
    mod.rs
    session_import.rs
    session_delete.rs
    workspace_skills.rs
    workspace_mcps.rs
```

说明：

1. `App.tsx` 和 `main.tsx` 可以继续留在 `src/` 根，避免为了目录美观多改入口配置。
2. `session/`、`workspace/` 是后端域根目录，不额外引入一层 `domains/`，避免过度抽象。
3. `commands.rs` 是域内入口，不再保留 `app.rs` 和 `manager.rs` 这类语义不清的根级大文件。

## 依赖约束

前端约束：

1. `src/app/` 可以依赖 `features/*` 和 `shared/*`。
2. `src/features/sessions/` 只能依赖 `shared/*` 和 `app/` 的稳定类型，不依赖 `features/workspace/`。
3. `src/features/workspace/` 只能依赖 `shared/*` 和 `app/` 的稳定类型，不依赖 `features/sessions/`。
4. `src/shared/*` 不允许依赖 `features/*`。
5. feature 私有样式不能再给 shared UI 提供基础类名。

后端约束：

1. `session/` 和 `workspace/` 各自维护自己的 model、command、service 代码。
2. `state/` 只依赖稳定类型，不反向依赖整个大业务模块。
3. `support/` 只放无业务语义的支持代码，不接收 session/workspace DTO。
4. `lib.rs` 只负责 Tauri builder、state 注册和 command handler 拼装，不再承载业务逻辑。

执行约束：

1. 一个阶段只做一个主题，不把多阶段混到同一批改动里。
2. 允许为了缩小改动面使用短期 re-export 或门面文件，但必须在下一阶段清理，不能长期保留。
3. 目录迁移阶段默认不改业务行为，除非顺手修复了明确的结构性 bug。
4. 每完成一个阶段，旧目录立即冻结，不再向旧路径新增同域代码。

## 阶段计划

## Phase 1：建立 app/shared 骨架并修正错误归属

### 目标

1. 把 app 级状态从 workspace 域中拆出去。
2. 把真正共享的 UI 和工具收敛到 `shared/`。
3. 切断 `DialogShell` 对 `session-detail.css` 的隐式样式依赖。
4. 为后续两个前端业务域收口打基础。

### 变更范围

| 当前文件 | 目标文件 |
| --- | --- |
| `src/components/top-mode-switch.tsx` | `src/app/components/top-mode-switch.tsx` |
| `src/components/top-mode-switch.css` | `src/app/components/top-mode-switch.css` |
| `src/components/app-select.tsx` | `src/shared/ui/app-select.tsx` |
| `src/components/app-select.css` | `src/shared/ui/app-select.css` |
| `src/components/app-toast.tsx` | `src/shared/ui/app-toast.tsx` |
| `src/components/app-toast.css` | `src/shared/ui/app-toast.css` |
| `src/components/confirm-dialog.tsx` | `src/shared/ui/confirm-dialog.tsx` |
| `src/components/confirm-dialog.css` | `src/shared/ui/confirm-dialog.css` |
| `src/components/dialog-shell.tsx` | `src/shared/ui/dialog-shell.tsx` |
| `src/lib/errors.ts` | `src/shared/lib/errors.ts` |
| `src/lib/format.ts` | `src/shared/lib/format.ts` |
| `src/lib/join-classes.ts` | `src/shared/lib/join-classes.ts` |
| `src/app-shell.css` 中 app 壳相关样式 | `src/app/styles/shell.css` |

新增文件：

1. `src/app/store.ts`：承载 `AppMode`、`useAppStore`。
2. `src/shared/ui/dialog-shell.css`：承载 `.dialog-*` 相关基础样式。

具体动作：

1. 把 `appMode` 和 `setAppMode` 从 `manager-store` 移到 `app/store.ts`。
2. 把 `AppMode` 从 `"sessions" | "manager"` 改成 `"sessions" | "workspace"`。
3. 更新 `App.tsx` 与 `TopModeSwitch`，全部改为使用 `app/store.ts`。
4. 把 `.dialog-backdrop`、`.dialog-card`、`.dialog-panel`、`.dialog-header`、`.dialog-body`、`.dialog-scroll`、`.dialog-actions` 等共享样式从 `session-detail.css` 抽到 `dialog-shell.css`。
5. 把 `app-root-shell`、`app-topbar`、`app-nav-shell` 这类真正属于 app 壳的样式移到 `src/app/styles/shell.css`。

完成判定：

1. `src/lib/manager-store.ts` 不再包含 `appMode` 和 `setAppMode`。
2. `src/components/session-detail.css` 不再包含 `.dialog-*` 共享样式定义。
3. `src/components/` 中不再包含 `top-mode-switch`、`app-select`、`app-toast`、`confirm-dialog`、`dialog-shell`。
4. `pnpm check` 通过。

## Phase 2：收口前端 sessions 域

### 目标

1. 让 sessions 相关代码在前端成为一个自洽模块。
2. 移除 `src/components/`、`src/hooks/`、`src/lib/` 对 session 域的散落承载。
3. 修正前端 `SessionDetail` 实际是 overview 的命名问题。

### 变更范围

| 当前文件 | 目标文件 |
| --- | --- |
| `src/features/session-workspace.tsx` | `src/features/sessions/workspace.tsx` |
| `src/components/session-list.tsx` | `src/features/sessions/components/session-list.tsx` |
| `src/components/session-list.css` | `src/features/sessions/components/session-list.css` |
| `src/components/session-detail.tsx` | `src/features/sessions/components/session-detail.tsx` |
| `src/components/session-detail.css` | `src/features/sessions/components/session-detail.css` |
| `src/components/session-detail-dialogs.tsx` | `src/features/sessions/components/session-detail-dialogs.tsx` |
| `src/components/source-switcher.tsx` | `src/features/sessions/components/source-switcher.tsx` |
| `src/components/source-switcher.css` | `src/features/sessions/components/source-switcher.css` |
| `src/hooks/use-session-timeline.ts` | `src/features/sessions/hooks/use-session-timeline.ts` |
| `src/lib/api.ts` | `src/features/sessions/api.ts` |
| `src/lib/session-store.ts` | `src/features/sessions/store.ts` |
| `src/lib/types.ts` | `src/features/sessions/types.ts` |
| `src/lib/source-app.ts` | `src/features/sessions/source-app.ts` |
| `src/app-shell.css` 中 sessions 布局样式 | `src/features/sessions/styles/workspace.css` |

具体动作：

1. 新建 `src/features/sessions/` 子目录结构，并完成 imports 收敛。
2. 把前端类型 `SessionDetail` 改名为 `SessionOverview`。
3. 保持 Tauri command 名称不变，先只改前端文件归属和本地类型名，不在这一阶段动 Rust 端命令实现。
4. 保持 sessions feature 对外入口尽量少，优先通过 `workspace.tsx` 作为装配入口。

完成判定：

1. `src/components/` 中不再包含 `session-*` 和 `source-switcher.*`。
2. `src/hooks/` 中不再包含 session 专用 hook。
3. `src/lib/` 中不再包含 session 业务代码。
4. `src/features/sessions/` 内部可以独立覆盖 session 页面所需的组件、样式、store、api、types。
5. `pnpm check` 通过。

## Phase 3：收口前端 workspace 域

### 目标

1. 用 `workspace` 取代前端 `manager` 作为业务域名。
2. 把 workspace 相关文件全部收口到 `src/features/workspace/`。
3. 这一阶段先做归位和重命名，不做大规模逻辑深拆。

### 变更范围

| 当前文件 | 目标文件 |
| --- | --- |
| `src/features/manager-workspace.tsx` | `src/features/workspace/workspace.tsx` |
| `src/features/manager-workspace-panels.tsx` | `src/features/workspace/panels.tsx` |
| `src/features/manager-workspace-dialogs.tsx` | `src/features/workspace/dialogs.tsx` |
| `src/features/manager-workspace-shared.ts` | `src/features/workspace/model.ts` |
| `src/lib/manager-api.ts` | `src/features/workspace/api.ts` |
| `src/lib/manager-store.ts` | `src/features/workspace/store.ts` |
| `src/lib/manager-types.ts` | `src/features/workspace/types.ts` |
| `src/styles/manager.css` | `src/features/workspace/styles.css` |

具体动作：

1. `ManagerTab` 改名为 `WorkspaceTab`。
2. `ManagerWorkspaceState` 改名为 `WorkspaceState`。
3. `ManagerConfigDocument` 改名为 `WorkspaceConfigDocument`。
4. `ManagerConfigView` 改名为 `WorkspaceConfigView`。
5. `ManagerInspection` 改名为 `WorkspaceInspection`。
6. `managerTab` 改名为 `workspaceTab`。
7. `manager-*` 文件名从前端彻底移除。
8. 继续保持 Tauri command string 暂不改名，这样前端只需要更新一个 `features/workspace/api.ts`。

完成判定：

1. `src/features/` 下不再存在 `manager-*` 文件。
2. `src/lib/` 下不再存在 `manager-*` 文件。
3. `src/styles/manager.css` 已删除，workspace 样式收口到 feature 内。
4. 前端代码层不再出现 `AppMode = "manager"`、`ManagerTab`、`ManagerInspection` 等旧命名。
5. `pnpm check` 通过。

## Phase 4：拆分 Rust 端 workspace 域并移除 `manager.rs`

### 目标

1. 解决 Rust 端最大的复杂度热点 `src-tauri/src/manager.rs`。
2. 把 workspace 域从“一个超大文件”拆成稳定子模块。
3. 把 Tauri command 入口的命名从 `manager` 迁到 `workspace`。

### 变更范围

源文件：

1. `src-tauri/src/manager.rs`
2. `src-tauri/src/manager/inspect.rs`
3. `src-tauri/src/lib.rs`
4. `src/features/workspace/api.ts`

目标模块：

1. `src-tauri/src/workspace/mod.rs`
2. `src-tauri/src/workspace/commands.rs`
3. `src-tauri/src/workspace/types.rs`
4. ~~`src-tauri/src/workspace/preferences.rs`~~（已废止，见下）
5. `src-tauri/src/workspace/document.rs`
6. `src-tauri/src/workspace/inspect.rs`
7. `src-tauri/src/workspace/targets.rs`
8. `src-tauri/src/workspace/skills.rs`
9. `src-tauri/src/workspace/mcps.rs`

建议拆分顺序：

1. 先抽 `types.rs`，把 `AgentTargetId`、view DTO、inspection DTO、mutation result、skill discovery snapshot 等稳定类型拿出来。
2. ~~再抽 `preferences.rs`，处理 `manager-preferences.json` 相关逻辑。~~（已废止：`manager-preferences.json` 从未落地，配置统一走 `config.yaml`，无需此模块。）
3. 再抽 `document.rs`，处理 workspace config 的 parse、validate、serialize、write。
4. 把现有 `manager/inspect.rs` 并入新的 `workspace/inspect.rs`，去掉 `use super::*;` 这种整体耦合。
5. 再抽 `targets.rs`、`skills.rs`、`mcps.rs`。
6. 最后新增 `commands.rs`，让 Tauri command 只保留参数解包和调用门面。

命令重命名规则：

1. 去掉 command 名里的 `manager` 前缀或中缀。
2. 例如 `get_manager_workspace_state` 改成 `get_workspace_state`。
3. 例如 `load_manager_workspace_state` 改成 `load_workspace_state`。
4. 例如 `set_manager_workspace_dir` 改成 `set_workspace_dir`。
5. 前端只允许在 `src/features/workspace/api.ts` 更新这些 command string，不在其他文件直接 `invoke`。

完成判定：

1. `src-tauri/src/manager.rs` 已删除。
2. `src-tauri/src/manager/inspect.rs` 已删除。
3. Rust 端不再存在 `manager` 根模块，workspace 域统一收口到 `src-tauri/src/workspace/`。
4. 前端不再调用任何 `*manager*` 命令字符串。
5. `pnpm check` 和 `cargo test --manifest-path src-tauri/Cargo.toml` 通过。

## Phase 5：拆分 Rust 端 session / state / support，并移除 `app.rs`、`shared.rs`、`state.rs`

### 目标

1. 让 Rust 端两大业务域最终对齐为 `session/` 和 `workspace/`。
2. 把 `shared.rs` 中混杂的模型和工具归位。
3. 让 `state` 只承载运行时状态，不反向依赖大模块。

### 变更范围

源文件：

1. `src-tauri/src/app.rs`
2. `src-tauri/src/shared.rs`
3. `src-tauri/src/state.rs`
4. `src-tauri/src/session/mod.rs`
5. `src-tauri/src/session/codex.rs`
6. `src-tauri/src/session/claude_code.rs`
7. `src-tauri/src/session/opencode.rs`

目标模块：

1. `src-tauri/src/session/commands.rs`
2. `src-tauri/src/session/model.rs`
3. `src-tauri/src/session/catalog.rs`
4. `src-tauri/src/session/timeline.rs`
5. `src-tauri/src/session/import.rs`
6. `src-tauri/src/session/delete.rs`
7. `src-tauri/src/session/text.rs`
8. `src-tauri/src/session/jsonl.rs`
9. `src-tauri/src/state/mod.rs`
10. `src-tauri/src/state/session_index.rs`
11. `src-tauri/src/state/skill_discovery.rs`
12. `src-tauri/src/support/fs.rs`
13. `src-tauri/src/support/paging.rs`
14. `src-tauri/src/support/time.rs`

具体动作：

1. 把 `app.rs` 的 Tauri commands 收到 `session/commands.rs`，让 `app.rs` 退出历史舞台。
2. 把 `shared.rs` 里的 session DTO 移到 `session/model.rs`。
3. 把 `SessionDetailOverview` 改名为 `SessionOverview`，和前端 Phase 2 对齐。
4. 把 `slice_page` 之类无业务语义的函数移到 `support/paging.rs`。
5. 把时间、文件这类稳定工具移到 `support/`。
6. 把 JSONL、标题提取、文本清洗等保留在 session 域，不再假装它们是全局 shared。
7. 把 `state.rs` 拆成 `state/session_index.rs` 和 `state/skill_discovery.rs`。
8. `SkillDiscoverySnapshot` 应依赖 `workspace/types.rs` 中的稳定类型，而不是让 `state` 直接依赖整个 workspace command 模块。
9. 移除 `lib.rs` 中的 `pub(crate) use shared::*`。

完成判定：

1. `src-tauri/src/app.rs` 已删除。
2. `src-tauri/src/shared.rs` 已删除。
3. `src-tauri/src/state.rs` 已删除。
4. `lib.rs` 中不再有 `pub(crate) use shared::*`。
5. `state/` 不再依赖整个 workspace 模块。
6. `pnpm check` 和 `cargo test --manifest-path src-tauri/Cargo.toml` 通过。

## Phase 6：清理临时兼容层、重组测试和补足文档

### 目标

1. 删除阶段性门面、re-export、旧路径兼容代码。
2. 让测试和文档也跟新的域结构一致。
3. 给后续继续演进留下可复用的边界说明。

### 具体动作

1. 删除所有仅用于迁移期的中转导出文件。
2. Rust 端把 `src-tauri/src/tests.rs` 整理到 `src-tauri/src/tests/`，按域分组。
3. 增补两份文档：
4. `docs/ai/decisions/` 下补一份命名表，说明 `sessions / workspace / targets / skills / mcps` 的边界。
5. `docs/ai/decisions/` 或 `docs/ai/context/` 下补一份前后端 contract 对照表，列出 session 和 workspace 的关键 DTO。

完成判定：

1. 仓库里不存在旧目录兼容层和仅为过渡保留的 re-export。
2. 前端不再从 `src/lib/` 读取任何业务代码。
3. Rust 根目录不再保留 `app.rs`、`manager.rs`、`shared.rs`、`state.rs`。
4. `pnpm check`、`cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm tauri build` 通过。

## 并行执行与依赖矩阵

### 硬依赖链

1. Phase 1 是 Phase 2 和 Phase 3 的硬依赖。
原因：`app/`、`shared/`、`AppMode`、`DialogShell`、app shell 样式边界必须先稳定，否则后续两个前端业务域会边迁移边反复改基础路径。

2. Phase 3 是 Phase 4 的硬依赖。
原因：Phase 4 会修改 `src/features/workspace/api.ts` 的 Tauri command 调用，如果前端还停留在 `manager-*` 命名和旧路径，会把前后端命令重命名和前端域迁移搅在一起。

3. Phase 4 是 Phase 5 的硬依赖。
原因：Phase 5 需要让 `state/skill_discovery` 依赖稳定的 `workspace/types.rs`，先拆完 workspace 域，后面的 `state` 脱耦才有稳定落点。

4. Phase 6 依赖前面全部阶段完成。
原因：它负责删除兼容层、重组测试和补文档，必须建立在结构已基本稳定的前提下。

### 软依赖

1. Phase 2 对 Phase 5 存在软依赖。
原因：Phase 2 会把前端 `SessionDetail` 改名为 `SessionOverview`，Phase 5 会把 Rust `SessionDetailOverview` 改名为 `SessionOverview`。两边不是绝对必须同一天完成，但命名最好按这个顺序统一，避免跨层名字来回改两次。

2. Phase 2 和 Phase 3 之间没有业务逻辑硬依赖。
原因：它们分别收口 `sessions` 和 `workspace`，逻辑上独立。
限制：它们都要改 `src/App.tsx`、共享 import 路径和顶层 feature 入口，所以如果多人并行开发，必须约定合并顺序和 rebase 责任。

### 可并行批次

| 批次 | 内容 | 前置依赖 | 可与谁并行 | 主要冲突点 | 合并要求 |
| --- | --- | --- | --- | --- | --- |
| 1A | `app/store.ts`、`TopModeSwitch`、`App.tsx` 中 app mode 迁移 | 无 | 1B | `src/App.tsx`、`AppMode` | 1A 合并后，1B/1C 统一 rebase |
| 1B | `shared/ui`、`shared/lib` 文件归位 | 无 | 1A | 各 feature 的 import 路径 | 1B 合并后再做 1C 最稳 |
| 1C | `dialog-shell.css` 抽离、app shell 样式抽离 | 1B 最好先完成 | 1A 理论可并行 | `dialog-shell.tsx`、`session-detail.css`、`app-shell.css` | 1C 必须基于 1B 最终路径落地 |
| 2A | Phase 2 全部前端 `sessions` 收口 | Phase 1 | 3A | `src/App.tsx`、共享 import | 如与 3A 并行，后合并者负责 rebase |
| 3A | Phase 3 全部前端 `workspace` 收口 | Phase 1 | 2A | `src/App.tsx`、共享 import、app mode 命名 | 如与 2A 并行，后合并者负责 rebase |
| 4A | Rust `workspace/types.rs`、`preferences.rs`、`document.rs`、`inspect.rs` 抽离 | Phase 3 | 无 | `workspace` 类型定义、`lib.rs` module 声明 | 4A 必须先于 4B、4C |
| 4B | Rust `targets.rs`、`skills.rs`、`mcps.rs` 抽离 | 4A | 无 | `workspace` 内部 helper、共享类型 | 4B 合并前不得改 command 名 |
| 4C | Tauri workspace commands 改名，前端 `features/workspace/api.ts` 跟进 | 4A、4B | 无 | `src-tauri/src/lib.rs`、`workspace/commands.rs`、`features/workspace/api.ts` | 4C 必须单独收口并跑完整回归 |
| 5A | Rust `session/model.rs`、`support/*`、`jsonl.rs`、`text.rs` 抽离 | Phase 4 | 5B 局部可并行 | `shared.rs` 内容拆分边界 | 5A 合并后再收口 5B 最稳 |
| 5B | Rust `session/commands.rs`、`state/*`、`lib.rs` 收口并删除 `app.rs/shared.rs/state.rs` | 5A、Phase 4 | 无 | `src-tauri/src/lib.rs`、state 依赖、command 注册 | 5B 需要单一 owner |

### 不建议并行的项

1. 不要把同一个 feature 的“路径迁移”和“类型重命名”拆给不同 agent 同时做。
原因：它们会同时改同一批 import 和同一批文件，合并冲突会非常重。

2. 不要把 Tauri command 重命名和前端 `invoke` 调用拆成两个独立批次。
原因：任何一边先落地都会导致运行时直接断。

3. 不要让多个批次同时改 `src-tauri/src/lib.rs`。
原因：这里是 command registry 和模块声明汇合点，必须有单一 owner 收口。

4. 不要让多个批次同时改 `src/App.tsx` 超过一次集成窗口。
原因：Phase 2 和 Phase 3 都要碰它，最好通过明确 merge order 解决，而不是长期并行悬挂。

### 推荐并行方式

如果后面要同步开工，推荐按下面方式分配：

1. 先完成 Phase 1，不并行跨 phase。
2. Phase 1 完成后，开两个并行流：
3. 一条做 Phase 2 的 `sessions` 前端收口。
4. 一条做 Phase 3 的 `workspace` 前端收口。
5. 两条都完成并合并后，再进入 Phase 4。
6. Phase 4 内部可以拆成 `4A -> 4B -> 4C` 三个串行子批次，不建议跨子批次并行合并。
7. Phase 5 内部可以让 5A 先抽模型和 support，5B 再统一删旧入口文件。
8. Phase 6 单独做收尾，不与其他 phase 并行。

## 并行执行下的测试策略

### 当前测试现实

1. 仓库当前已有稳定的 TypeScript 编译检查和 Rust 测试命令。
2. 仓库当前没有现成的前端单测框架。
3. 这轮拆分的主要风险是“路径、命名、样式、命令注册”断裂，不是复杂算法错误。
4. 所以这轮以 `pnpm check`、`cargo test`、Tauri 手工 smoke check 为主，不为了目录迁移单独引入新的前端测试框架。

### 测试分层

1. 批次级测试：每个并行批次自己先跑最小必要测试，保证分支本身可集成。
2. phase 级测试：同一 phase 全部批次合并后，再跑该 phase 的完整回归。
3. 集成级测试：两个并行流合并回主线后，再跑一次跨域回归，确认没有在 `App.tsx`、shared import、command registry 上产生集成问题。
4. 最终发布级测试：全部 phases 完成后，跑构建和完整 smoke。

### 批次最小测试矩阵

| 批次类型 | 必跑命令 | 必做 smoke |
| --- | --- | --- |
| 仅前端路径迁移 | `pnpm check` | 打开受影响页面，确认能加载，不白屏 |
| 前端共享 UI / 样式迁移 | `pnpm check` | 验证 top mode switch、dialog、confirm、toast、sidebar 样式 |
| Rust 内部模块抽离，不改 command 名 | `pnpm check`、`cargo test --manifest-path src-tauri/Cargo.toml` | 打开对应页面，确认基础加载正常 |
| Tauri command 改名 / registry 改动 | `pnpm check`、`cargo test --manifest-path src-tauri/Cargo.toml` | 从 UI 走完整业务链路，确认每个命令还能打通 |
| 删除旧入口文件或兼容层 | `pnpm check`、`cargo test --manifest-path src-tauri/Cargo.toml`、必要时 `pnpm tauri build` | 全量 smoke，尤其关注启动和页面切换 |

### 分阶段重点回归

| Phase | 必测功能 |
| --- | --- |
| Phase 1 | 顶部模式切换、App 壳布局、DialogShell、ConfirmDialog、Toast |
| Phase 2 | detect sources、会话列表、详情加载、分页、导入预览、删除弹窗 |
| Phase 3 | workspace 加载、tabs 切换、targets/skills/mcp 三类面板和 dialogs |
| Phase 4 | 从 workspace UI 触发的所有 Tauri commands：workspace 选择、读取配置、target CRUD、skill discovery/import/sync、mcp preview/apply/remove |
| Phase 5 | 从 sessions UI 触发的所有 Tauri commands：detect sources、refresh sessions、list/detail/messages/events、import、delete |
| Phase 6 | 全量回归：两大模式切换、所有主要 dialogs、核心导入/分发链路、最终构建 |

### 新增测试约束

1. Rust 端凡是从大文件里抽出的纯函数、parser、format helper，如果原来没有直接测试，抽出后优先在对应新模块旁边补单元测试。
2. Phase 4 和 Phase 5 这两类会改 Tauri command 或 registry 的批次，必须在 UI 手工跑完对应完整链路，不能只看 `cargo test` 通过。
3. 两条并行流合并回主线后，必须重新跑一次 phase 级测试，不能只相信各自分支上的批次级测试。
4. 任何修改 `src/App.tsx`、`src-tauri/src/lib.rs`、共享样式文件的批次，都要额外做一次启动 smoke check。

## 每阶段验收清单

### 通用静态检查

1. `pnpm check`
2. Rust 变更阶段额外执行 `cargo test --manifest-path src-tauri/Cargo.toml`
3. 最终阶段执行 `pnpm tauri build`

### 通用人工 smoke check

1. 顶部模式切换可以正常在“历史会话”和“配置与分发”之间切换。
2. 历史会话页可以正常加载 sources、列表、详情、分页、导入预览、删除弹窗。
3. workspace 页可以正常加载 workspace、切换 tabs、打开主要 dialogs、查看 inspect 结果。
4. 样式没有因为目录迁移出现明显丢失，尤其是 dialogs、toast、sidebar、workspace panels。

### 关键 grep 断言

阶段完成后应逐步满足下面这些断言：

1. Phase 1 后：`src/components/` 不再包含 `top-mode-switch`、`dialog-shell`、`confirm-dialog`、`app-select`、`app-toast`。
2. Phase 2 后：`src/lib/`、`src/hooks/`、`src/components/` 不再承载 session 业务代码。
3. Phase 3 后：前端不再出现 `manager-*` 文件和 `"manager"` 模式值。
4. Phase 4 后：Rust 根目录不再有 `manager.rs`。
5. Phase 5 后：Rust 根目录不再有 `app.rs`、`shared.rs`、`state.rs`。

## 风险与规避

1. 风险：目录移动和命名重命名同时发生，容易让 diff 过大。
2. 规避：每个 phase 只围绕一个域或一个结构主题，不跨阶段混做。

3. 风险：Tauri command string 改名会影响前端调用链。
4. 规避：先把 frontend API wrapper 收口到 `features/workspace/api.ts` 和 `features/sessions/api.ts`，再做 command rename。

5. 风险：抽样式时容易漏掉共享 class，导致 dialogs 或 buttons 样式丢失。
6. 规避：Phase 1 必须先抽 `dialog-shell.css`，并在 smoke check 中专门验证 dialogs、confirm、toast。

7. 风险：Rust 拆模块时容易形成循环依赖。
8. 规避：先抽稳定 `types.rs`，再抽 commands、document、skills、mcps，不要先乱拆函数实现。

9. 风险：为了平滑迁移留下长期兼容层，最后新旧结构并存。
10. 规避：每个 phase 明确“完成判定”，下一个 phase 开始前先删掉上一阶段留下的临时门面。

## 执行顺序结论

推荐严格按下面顺序推进，不要跳步：

1. Phase 1：建立 `app/` 和 `shared/` 基础边界。
2. Phase 2：收口前端 `sessions`。
3. Phase 3：收口前端 `workspace`。
4. Phase 4：拆 Rust `workspace`，移除 `manager.rs`。
5. Phase 5：拆 Rust `session / state / support`，移除 `app.rs`、`shared.rs`、`state.rs`。
6. Phase 6：清理兼容层、重组测试、补足文档。

这个顺序的原因是：

1. 先从前端边界和低风险目录收口开始，快速建立新的命名和放置规则。
2. 再动 Rust 端两个最大的复杂度热点，避免一开始同时承受 TS 和 Rust 两侧的大规模拆分风险。
3. 最后再统一清理兼容层和测试，避免中途反复重写测试路径。
