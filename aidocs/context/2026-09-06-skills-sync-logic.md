# agent-tools Skills 同步逻辑

本文以当前源码为准，说明 Skills source 如何被发现、缓存并同步到 targets。这里的“同步”只指 Skills 目录分发，不包括 MCP 配置分发。

## 1. 核心模型

### 1.1 Source

source 是配置中的一个 Skills 来源，也是同步时解析出的“来源根目录”。配置文件为：

```text
<系统 config 目录>/agent-tools/config.yaml
```

`WorkspaceConfigStore::app()` 使用系统 config 目录；在 macOS 上通常是 `~/Library/Application Support/agent-tools/config.yaml`，其他平台由 `dirs::config_dir()` 决定。

当前支持两种 source：

| 类型 | 配置字段 | 同步时的来源根目录 |
| --- | --- | --- |
| `local` | `root_path` | 配置指定的本地目录 |
| `git` | `repo`、可选 `ref` | 持久化 git cache 目录 |

local 路径支持绝对路径、`~/...`，以及相对 `config.yaml` 所在目录的路径。git source 不直接从工作区副本读取；先确保仓库位于 cache，再从 cache 发现 Skills。

source 配置还包含：

- `include_name_patterns`：按 Skill 目录名过滤。
- `include_path_patterns`：按相对 source 根目录的 Skill 路径过滤。

导入 source 只写入 `config.yaml`，不会创建 target 中的链接；实际分发由之后的同步操作完成。

source ID 在单个导入和批量 Git 导入时都由后端生成 UUID。配置只保存 source 定义和 include patterns，不保存“本次选中了哪些 Skill”；逐个 Skill 的选择是一次同步请求的临时输入。

### 1.2 Target

target 是 agent 工具实际扫描 Skills 的目录。全局 target 使用普通 ID，项目 target 使用 `project-id:agent-id` 复合 ID。两者最终都通过同一种目录链接方式写入。

内置 target 的默认 Skills 目录：

| agent | 全局 target | 项目 target |
| --- | --- | --- |
| Codex | `~/.agents/skills` | `<project>/.agents/skills` |
| Claude | `~/.claude/skills` | `<project>/.claude/skills` |
| OpenCode | `~/.config/opencode/skills` | `<project>/.opencode/skills` |
| ZCode | `~/.zcode/skills` | `<project>/.zcode/skills` |

target 可以在 `config.yaml` 中自定义 `skill_dir`。target 的 `enabled=false` 时不能在同步弹窗中选择，后端同步也会跳过它。

### 1.3 安装状态的权威来源

当前没有 workspace Skills 副本目录，也没有 Skills 安装注册表。安装状态只由 target Skills 目录中的实际目录链接表示：

```text
<target.skill_dir>/<skill.name>  ->  <source.root>/<skill.relative_path>
```

因此 `config.yaml` 只描述 source 和 target，不能单独证明某个 Skill 已经同步。

## 2. Skill 发现与过滤

发现逻辑递归扫描 source 根目录下所有名为 `SKILL.md` 的文件。每个文件对应一个 Skill，记录：

- `name`：包含 `SKILL.md` 的父目录名。
- `relative_path`：该 Skill 目录相对于 source 根目录的路径。
- source 文件路径和 Skill 目录路径。

发现结果按 `relative_path` 排序。

patterns 的规范化只做两件事：去除首尾空白、删除空字符串；不会改写 pattern。匹配规则如下：

- `*` 匹配任意长度字符串。
- `?` 匹配一个字符。
- name patterns 为空表示不限制名称；path patterns 为空表示不限制路径。
- 两个维度都存在时必须同时匹配（AND）。同一维度内任一 pattern 匹配即可（OR）。

同步弹窗展示全部发现结果，并把匹配项默认勾选、未匹配项折叠展示。用户仍可以手动选择未匹配项。

## 3. Git cache

### 3.1 目录和身份

git cache 位于：

```text
<系统 config 目录>/agent-tools/git/<owner>/<repo>/
```

支持 HTTPS、SSH、scp-style（例如 `git@host:owner/repo`）以及简单的 `owner/repo` 形式。路径使用解析出的最后两个路径段作为 `owner/repo`，不再使用 hash；同一 repo 的不同 source/ref 共用同一个 cache。

### 3.2 首次 clone 和自动刷新

- cache 不存在时执行 `git clone`；有 ref 时使用 `--branch <ref>`。
- cache 已存在时，普通发现/同步只在以下情况刷新：
  - 距离上次成功刷新达到 24 小时；
  - 请求的 ref 与记录不一致；
  - 刷新记录不存在；
  - 系统时间早于上次刷新时间。
- 刷新命令依次为 `git fetch origin`、可选 `git checkout <ref>`、`git pull --ff-only`。
- 刷新记录为 `<cache>/.git/agent-tools-last-fetch`：文件内容保存 ref，修改时间用于展示和 24 小时判断。
- source 行的“强制拉取”使用 Force 策略，绕过 24 小时限制。

如果 cache 路径存在但不是 git worktree（没有 `.git`），会先删除该路径，再按全新 clone 处理。

同一 repo 的不同 source/ref 共用一个 cache 工作树，而不是为每个 ref 保留独立副本。请求不同 ref 时会在这个目录内 checkout 到新 ref；因此后一次发现/同步看到的是最近一次切换后的工作树内容。

## 4. 同步目标选项和 UI 状态

`get_sync_target_options` 同时枚举全局 targets 和所有项目 agents。每个 target 返回 ID、显示路径、启用状态以及当前目录链接列表。

为展示 target 关联状态，后端只解析已有 source 根路径和 target 内的目录链接，不会因为列出选项而主动 clone 或刷新 Git cache。

### 4.1 整目录继承

如果某个 target 的整个 `skill_dir` 是目录链接，并且解析后指向另一个受管 target 的 `skill_dir`：

- UI 标记 `linkedTargetId`，禁用该 target 的选择；
- 全选计数不包含它；
- 不重复扫描其内部 Skills 链接，避免同一批物理链接被报告两次。

指向受管 target 之外目录的 `skill_dir` 链接仍可选择。后端对项目 target 还会再次检查整目录链接并跳过写入。

### 4.2 target 内链接状态

target 只扫描 Skills 目录的直接子项，不递归扫描 Skill 内容。每个目录链接会根据解析后的目标路径分类：

| 状态 | 含义 |
| --- | --- |
| `linked` | 目标存在 `SKILL.md`，且相对 source 路径匹配该 source 的 include patterns |
| `excluded` | 目标属于某个已配置 source，但当前 source 的 include patterns 不再匹配 |
| `sourceMissing` | 目标路径落在某个 source 根目录下，但目标 Skill 目录或 `SKILL.md` 不存在 |
| `unmanaged` | 目标路径不属于任何已配置 source |

source 根路径重叠时，一个链接可能匹配多个 source；文件系统无法证明它最初由哪个 source 创建，因此接口返回 `matchedSourceIds`，不强行选择唯一归属。

清理逻辑按“链接目标位于 source 根目录下”的路径前缀判断，而不是按 source ID 记录归属。嵌套 source 根目录因此可能互相命中：删除或同步外层 source 时，也可能清理指向其子目录的链接。

## 5. 普通同步流程

入口为后端 `sync_source_to_targets`，核心实现是 `sync_source_to_targets_inner`；前端编排位于 `useSourceSync` 和 `SourceSyncDialog`。

### 5.1 弹窗阶段

1. 加载 target 选项和 source 的 Skill 选项。
2. Skills 选项包含全部发现结果及 `matched` 标记；默认选择 `matched=true` 的 Skill。
3. 用户选择 Skill（传 `relative_path`）和 target ID。
4. 点击同步后，前端先调用冲突预览。
5. 若存在冲突，弹出覆盖确认；确认后把 `overwriteExisting=true` 重新提交。
6. 弹窗加载时得到的 `sourceRoot` 和 Skill 列表作为 snapshot 传给后端。后端会验证 snapshot 根目录仍属于当前 source，并优先复用它，避免确认阶段重复 clone/扫描。

### 5.2 后端执行顺序

1. 读取 source 配置并解析 source 根目录：local 使用本地目录，git 使用 cache。
2. 根据用户传入的 `skill_paths`，按 `relative_path` 从发现结果中选出 Skills；不存在于发现结果的路径不会被处理。
3. 只对用户选择的 target 做冲突检查。禁用 target、不存在的 target，以及项目级整目录继承 target 不产生可覆盖冲突；正常 UI 流程不会把被标记为 `linkedTargetId` 的 target 传给后端。
4. `overwriteExisting=false` 且存在冲突时直接失败，不修改目标目录。
5. 清理**所选 targets 中**所有解析后指向当前 source 根目录的旧目录链接。未选择的 target 不会被清理。
6. 对每个所选 target 和 Skill，写入目标路径 `<target.skill_dir>/<skill.name>`：
   - 不存在：创建目录链接，结果 action 为 `create`；
   - 已经指向同一 source Skill：保持不变，不重复写入；
   - 指向其他路径，或是同名真实目录/文件：覆盖前删除，再创建目录链接，结果 action 为 `replace`。
7. target 不存在或已禁用时加入 warning 并跳过；写入过程中发生的文件系统错误直接返回失败。

同步结果包含：

- `removed`：第 5 步清理的链接；
- `applied`：新建或替换的链接；
- `warnings`：跳过的 target 等非致命信息。

### 5.3 目标路径和名称冲突

目标落点只使用 Skill 的 `name`，不保留 `relative_path`。因此同一 source 中不同路径下的同名 Skill 会竞争同一个目标路径：

```text
source/a/tool/SKILL.md  -> target/tool
source/b/tool/SKILL.md  -> target/tool
```

发现结果按相对路径排序，后处理的同名 Skill 会替换先处理的链接。patterns 不能消除这个目标名称冲突时，应由用户在同步选择阶段避免同时选择。

## 6. 移除同步

`remove_source_sync(source_id, target_ids)` 只接收 source 和 target，不需要选择具体 Skill。

- 仅扫描传入的 targets；
- 删除其中所有解析后指向该 source 根目录的目录链接；
- 不删除 source 文件、local 原始目录、git cache；
- 不删除真实目录、普通文件，或指向其他位置的链接；
- 没有可删除链接时返回空的 `removed` 结果。

因此普通同步和移除同步都遵循“只影响勾选 targets”的边界；只有删除 source 才会扫描全部 targets。

## 7. 编辑和删除 source

### 7.1 编辑 source

`update_skill_source` 只更新 `config.yaml` 中的：

- Git source 的 `ref`；
- local/git source 的 name/path include patterns。

它不会自动 clone、同步、撤回或修改现有 target 链接。编辑后只有再次同步，才会按新的选择和 patterns 清理、重建所选 targets 的链接。

### 7.2 删除 source

`delete_skill_source`（以及批量删除）执行以下操作：

1. 在配置锁内从 `config.yaml` 删除 source 条目，并原子写回配置。
2. 扫描所有全局和项目 targets，删除解析后位于该 source 根目录下的目录链接。
3. 如果 source 是 git：仅当没有其他 source 引用相同 repo 字符串时，删除对应的 `<owner>/<repo>` cache，并尽力清理空的父目录。

删除不会触碰：

- local source 的原始目录；
- target 中的真实目录或普通文件；
- 指向其他位置的软链接；
- 仍被其他 source 引用的共享 git cache。

## 8. 相关代码入口

后端：

- `src-tauri/src/workspace/skills.rs`
  - `discover_source_skills_raw`
  - `build_sync_skill_options`
  - `build_sync_target_options`
  - `sync_source_to_targets_inner`
  - `preview_source_sync_conflicts_inner`
  - `remove_source_sync_inner`
  - `remove_source_symlinks_from_targets_with_root`
  - `update_skill_source_inner`
  - `delete_skill_source_inner`
  - `refresh_git_repo_cache`
- `src-tauri/src/workspace/mod.rs`
  - `parse_manager_config`
  - `expand_path`
  - `normalize_patterns`
  - `resolve_target_from_id`
- `src-tauri/src/workspace/config.rs`
  - `WorkspaceConfigStore`
  - `git_cache_dir_for_repo`
- `src-tauri/src/workspace/types.rs`
  - `SourceSyncInput`、`SourceSyncResult`
  - `SyncSkillOption`、`SyncTargetOption`
  - `SkillLinkAssociation`、`SkillLinkState`

前端：

- `src/features/workspace/composables/useSourceSync.ts`：同步、冲突覆盖、移除同步的请求编排。
- `src/features/workspace/components/dialogs/SourceSyncDialog.vue`：加载 snapshot、选择 Skills/targets、发起同步。
- `src/features/workspace/components/SyncTargetGroups.vue`：target 分组、继承关系和链接状态展示。
