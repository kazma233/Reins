# skill 分发的 target 安装面收成深模块

- 日期：2026-10-12
- 状态：已实施（对应 `context/2026-10-12-architecture-review-backlog.md` 队列第 2 项 C1）

## 背景

「把一个 skill 装进一个 target」的物理语义散在四条平行路径里（`workspace/skills.rs`）：

1. 同步应用（`sync_source_to_targets_inner`）：遍历目标 → 判 enabled / 项目继承 → 算落点 → 分类 → 删旧链 → 建链；
2. 冲突预演（`source_sync_conflicts`）：另一份遍历，判 enabled / 项目继承 → 算落点 → 另一套分类；
3. 链接枚举（`scan_target_skill_links` + `classify_skill_link`）：再读一遍目录、再分一次类；
4. 按来源卸载（`remove_source_symlinks_from_targets_with_root`）：又一份遍历 + 又一份「哪条链接属于本来源」的判定。

同一件事被写了两遍以上：

- skip 规则两份且行为不同：同步对「不存在 / 已禁用」发警告、对「项目级整目录继承」静默跳过；预演只静默跳过，不产生警告；
- 落点计算 `target.skill_dir.join(&skill.name)` 出现在同步与预演两处；
- 「落点是否已经指进本来源」有两套口径：安装判定走规范化后的精确相等（`classify_skill_destination`），预演走 `starts_with` 前缀（`non_source_destination_detail`），两者对「指向同来源另一个 skill 的链接」结论不同；
- 路径规范化 `normalize_link_path` 与「相对来源根」`relative_to_source_root` 在两侧各有一份，历史 bug（/var → /private/var 下链接删不掉）正落在这种重复上。

## 决策

新增 `workspace/skill_install.rs`，把安装面收成一个模块，四个动作 + 一个状态机：

1. **目标判定单点**：`TargetSkip`（Missing / Disabled / LinkedProjectDir）+ `resolve_ids` / `managed_targets` 统一回答「这个目标参不参与分发」。两道门（enabled、项目级整目录继承）只判一次，各调用方只决定呈现（警告 / 静默）。
2. **落点与动作在 `SkillInstall`**：`apply`（安装）、`conflicts`（预演）、`remove_installed`（按来源卸载）、`remove_at`（单条卸载）。落点计算 `skill_dir.join(name)`、父目录创建、覆盖前的删除顺序都内化，调用方不再各写一遍。
3. **链接分类是唯一状态机**：`DestinationState` 区分 uninstalled / 指向本 skill / 指向同来源别的 skill / 指向别处 / 真实目录 / 真实文件 / 真实路径。`conflicts`、`apply`、UI 只读呈现都从它派生，不再各自比较路径。
4. **路径比较单一口径**：`normalize_link_path` / `relative_to_source_root`（含 Windows 扩展长度前缀与大小写规则）移入模块，`skills.rs` 的链接归属分类改为复用，杜绝两侧规范化不一致。
5. **测试夹具单 target**：模块内联测试用三行 `ResolvedTargetConfig` + 一个真实源目录直接调用四个动作，不再手拼整份 `config.yaml`。

### 明确保留的行为差异

「指向同来源另一个 skill 的链接」在安装侧是冲突（位置不对，会阻断同步并走替换），在 UI 只读呈现里是 conflict，在按来源卸载里会被删除（它是本来源的残留）。这三处现在是同一状态机的三个消费点，不是三份规则。

### 一处行为收紧（有意为之）

按来源卸载此前只按 target id 过滤，不看目标状态；启用与跳过的判定在同步与预演里各写一遍，卸载路径没有。重构后卸载同样经 `resolve_ids`，**已禁用**与**项目级整目录继承**的目标不再被清理。

影响面：弹窗里禁用目标本就不可选，用户显式勾选到禁用目标才会经过这条路径（改动前能清、改动后跳过）。选择收紧是因为「禁用 = 不参与分发」必须与安装侧一致，否则会出现「清掉了但装不进」的不对称。

### 隐性前置被显式化

安装前创建目标父目录原先只存在于同步路径的循环里；重构后由 `SkillInstall::apply` 负责（`create_dir_all`），这是首个用例失败后补回的行为——记录在此，避免下次再当成无关细节删掉。

## 证据与验证

- 行为等价：改造前后全量测试同为 424 passed / 0 failed（新增 5 个模块测试后为 429）；既有 workspace 用例（链接生命周期、按来源清理、目标预设、冲突弹窗）逐条保持。
- 新增直接测试：`workspace/skill_install.rs` 的 `install_creates_link_then_reports_unchanged`、`missing_and_disabled_targets_are_skipped`、`real_directory_is_reported_as_conflict_then_replaced`、`link_to_other_skill_of_same_source_is_conflict_and_removed`、`remove_installed_keeps_other_source_links`。
- 重复消除：`skills.rs` 1326 行（改造前约 1693 行）；`SkillAction` / `SkillActionKind` 与本地路径助手删除；`normalize_link_path` 等三份实现合并为一份。
- 覆盖缺口（诚实记录）：模块的 skip 判定与 Windows 分支依赖 `#[cfg(windows)]` 用例，本机无法执行；Windows 上 `relative_to_source_root` / `windows_path_component_eq` 是逐字搬运（未改语义），行为应以 CI 的 Windows 基线为准。
