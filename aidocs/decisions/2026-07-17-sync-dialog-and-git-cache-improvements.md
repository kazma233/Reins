# 同步弹窗能力扩展与 git cache 改进

- 日期：2026-07-17
- 状态：生效中
- 关联：`2026-07-16-symlink-only-no-workspace-container.md`（在其基础上修订）

## 背景

同步来源弹窗（`SourceSyncDialog`）落地后，实际使用中暴露出几个问题：

1. **无法取消同步**：弹窗只有「开始同步」按钮，skills 选了至少 1 个 + targets 选了至少 1 个才能点。一旦同步过就无法在弹窗里清空某个 target 里该 source 的软链——只能去 target 目录手动删。
2. **软链 target 被误选**：某个 target 的 skillDir 如果是指向另一个 managed target skillDir 的软链（典型场景：项目 `.agents/skills` 软链到全局 `~/.agents/skills`），同步过去没有意义（会被软链覆盖语义吞掉），但 UI 照样允许勾选。
3. **git cache 路径不可读**：git 源 clone 到 `git/<source_hash>/`，hash 是 `DefaultHasher` 算的 16 位 hex（如 `a3f9c2b1e8d04f17`），排查时根本看不出是哪个仓库。而且 `DefaultHasher` 跨 Rust 版本不保证稳定，升级编译器后可能产生孤儿缓存。
4. **每次同步都全量 re-clone**：即使 cache 已存在也先删后 `git clone --depth 1`，大仓库同步慢。
5. **同步弹窗整体滚动**：skills 多或 targets 多时，整个弹窗 body 一起滚，两列没法各自独立看。
6. **同步完成后按钮卡在禁用**：`onConfirm` 是 async 但子组件没 await，`confirming` 不复位；加上 reload 阻塞了弹窗关闭。

## 决策

### 1. 新增「移除同步」操作（`remove_source_sync`）

- 同步弹窗 actions 区加红色「移除同步」按钮，与「取消」「开始同步」并列。
- 只需勾选 targets（不要求选 skills），点击后清空**勾选的 targets**里该 source 安装的全部软链。
- 新增后端命令 `remove_source_sync(source_id, target_ids)`，复用 `remove_source_symlinks_from_targets`（带 target_ids 过滤）。

### 2. 同步/移除的操作范围严格限定在勾选的 targets

- `sync_source_to_targets_inner` 步骤 3（删除旧软链）从「扫所有 target」改为「只扫勾选的 target」。
- `remove_source_symlinks_from_targets` 加 `target_ids: Option<&HashSet<&str>>` 参数：`Some` 只清指定 target，`None` 清所有（仅 `delete_skill_source` 用）。
- 语义统一：用户没勾选的 target，同步/移除都不会动。只有删除 source 才会扫所有 target 清软链。

### 3. 软链 target 自动禁用并展示指向

- `build_sync_target_options` 对每个 target 的 skillDir 调用 `read_skill_dir_link` 检测软链，解析后与其他 target 的 skillDir 规范化路径比对。
- 仅当软链指向**另一个 managed target**时才禁用（`SyncTargetOption.linkedTargetId`），指向外部目录的软链正常可选。
- 禁用 target：checkbox disabled、不预选、不参与全选、Targets 计数分母不含它、label 变灰、展示「指向 {targetId}」提示。

### 4. git cache 改用 `<owner>/<repo>` 布局 + 定时增量 pull

- 新增 `parse_git_owner_repo(repo)` 解析 https / ssh / scp-style / 带端口等各种 URL，提取 owner/repo（去 `.git`）。
- cache 路径从 `git/<source_hash>/` 改为 `git/<owner>/<repo>/`，路径可读、跨版本稳定。
- 不存在时才 `git clone`；已有 cache 默认在距离上次成功更新达到 24 小时、Git ref 改变、刷新记录缺失或系统时间回退时，才执行 `git fetch` + `git checkout <ref>` + `git pull --ff-only`。去掉 `--depth 1`（切分支场景下浅克隆拉不到目标分支）。
- 记录存放在 cache 的 `.git/agent-tools-last-fetch`：内容为成功刷新时使用的 ref，文件修改时间为刷新时间，不写入 `config.yaml`。Git 来源行提供 `强制拉取`，可绕过 24 小时间隔。
- 同一 repo 不同分支共用一个 cache 目录（用户明确选择）。
- 删除 source 时，仅当该 repo 没被其他 source 引用才删 cache。
- 移除 `git_skill_source_hash` / `source_hash` / `DefaultHasher` 相关代码。

### 5. 同步弹窗左右两列各自独立滚动

- `.manager-sync-dialog` 作用域下把 `dialog-body → dialog-scroll → dialog-scroll-content` 三层改成 flex 高度传递桥（`flex column + min-height:0`），取消 `.dialog-scroll` 的整体 `max-height` 和滚动。
- 高度从 `dialog-card`（`max-height:88vh`）一路传到 `.manager-sync-columns`，skills 列表和 targets 列表的 `overflow-y:auto` 各自生效。
- 弹窗整体不滚动。

### 6. 同步按钮状态修复

- `SourceSyncDialogProps.onConfirm` 签名改为 `(...) => void | Promise<void>`，子组件 `Promise.resolve(...).finally(() => setConfirming(false))` 在异步完成后复位。
- `handleConfirmSourceSync` 成功路径调整为 sync → showNotice → 立即关弹窗 → reload 放后台（`preserveNotice: true`），不再让 reload 阻塞弹窗关闭。

## 理由

- **移除同步填补「取消分发」的空缺**：之前同步是单向的，用户没有正规途径清掉某个 source 在特定 target 的软链，只能手动去文件系统删。移除同步让分发可逆。
- **限定在勾选 targets 是更直觉的语义**：用户勾选了 A、B 两个 target 同步，不应该影响 C。旧实现「步骤3 扫所有 target」是历史遗留，会让未勾选的 target 里该 source 的旧软链也被清掉，与用户预期不符。
- **软链 target 禁用避免无意义操作**：软链 target 的 skillDir 指向另一个 target，同步过去的内容会被软链语义覆盖或无效化，禁用并提示指向关系是最直接的引导。
- **owner/repo 布局可读且稳定**：`DefaultHasher` 跨版本不稳定是已知隐患，owner/repo 既可读又确定。
- **定时增量 pull 减少重复拉取**：打开界面、查看同步列表等高频操作通常只读本地 cache；过期或用户手动触发时才访问远端，避免大仓库反复消耗带宽和时间。

## 影响

### 新增命令

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `remove_source_sync` | `source_id: String, target_ids: Vec<String>` | `SourceSyncResult` | 清空指定 targets 里该 source 的软链 |

### 变更的 DTO

- `SyncTargetOption` 新增 `linkedTargetId: Option<String>`（指向的 managed target id，非软链或指向外部时为 None）。

### 路径变化（破坏性）

- git cache：`git/<source_hash>/` → `git/<owner>/<repo>/`。旧的 hash 目录不会被新逻辑识别，需手动清理。重新导入会 clone 到新位置。

### 代码落点

- `src-tauri/src/workspace/mod.rs`：`parse_git_owner_repo`、`git_cache_dir_for_repo`、`clone_git_repo_to_cache`（重写）、`remove_source_symlinks_from_targets`（加 target_ids）、`remove_source_sync_inner`、`build_sync_target_options`（软链检测）
- `src-tauri/src/workspace/commands.rs`：`remove_source_sync` 命令
- `src-tauri/src/workspace/types.rs`：`SyncTargetOption.linked_target_id`
- `src-tauri/src/lib.rs`：注册 `remove_source_sync`
- `src/features/workspace/types.ts`：`SyncTargetOption.linkedTargetId`
- `src/features/workspace/api.ts`：`removeSourceSync`
- `src/features/workspace/dialogs.tsx`：移除同步按钮、软链 target 禁用渲染、`onRemoveSync` prop、按钮状态修复
- `src/features/workspace/workspace.tsx`：`handleRemoveSourceSync`、`handleConfirmSourceSync` 重构
- `src/features/workspace/styles.css`：两列独立滚动布局、软链提示与禁用样式

### 单测

- `parse_git_owner_repo_handles_common_url_forms`：6 种 URL 格式
- `parse_git_owner_repo_rejects_malformed`：空串、单段名
- `git_cache_dir_for_repo_uses_owner_name_layout`：目录结构断言
