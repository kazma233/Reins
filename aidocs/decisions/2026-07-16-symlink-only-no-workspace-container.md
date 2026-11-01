# 废弃 workspace 容器，统一软链化并改用全局配置

- 日期：2026-07-16
- 状态：生效中

## 背景

在本次重构之前，nyleen 的「配置与分发」以 **workspace 容器**为基本单位：

1. 每个 workspace = 一个目录 + 一个 `nyleen.yaml` + 一个 `skills/` 子目录。
2. `nyleen.yaml` 是 workspace 级配置文件，其中包含 `workspace_name` 字段。
3. Skills 同步走**两段式 + 真 copy**：
   - 源（local 目录 / git 仓库）→ 真实 copy 到 `<workspace>/skills/<hash>/<id>/`。
   - 再从该副本软链到 target。
4. git clone 没有固定落点，clone 出来的内容散落在临时位置，没有统一缓存策略。
5. skill 条目带 `workspace_path` 与 `missing` 字段。

这套模型存在几类问题：

- **空间浪费**：每个 skill 在 workspace 下都要保留一份真实副本，多个 workspace 之间、多次同步之间都会产生重复拷贝。
- **复杂度高**：「源 → 副本 → target」是两段式流程，副本既是缓存又是分发锚点，导致同步、删除、inspect 多条链路都要围绕副本目录做额外维护（清空目录、自下而上收敛空壳等）。
- **git cache 混乱**：git 源的 clone 结果没有统一、可预测的存放路径，难以追溯，也无法在删除 source 时一并清理。
- **文档与实现不一致**：旧 README 长期声称「全局 target → 软链接；project target → 真 copy」，但实际代码一直是统一软链，文档误导了维护者。
- **多 workspace 概念冗余**：实际只需要一份配置就能描述全部 sources/targets，workspace 容器徒增了选择与目录管理负担。

## 决策

### 1. 废弃 workspace 容器，改用全局单一配置

- 不再有 workspace 容器的概念。
- 全局唯一配置文件：`~/Library/Application Support/nyleen/config.yaml`。
- 该文件由程序维护，用户也可手动编辑，承载 `sources` / `targets` / `projects` / `mcps` 的定义；不保存 skill 安装注册表。
- `config.yaml` 不再包含 `workspace_name` 字段。

### 2. skills 同步机制：全面软链化，不再有任何真 copy

- 源 → target **直接软链**，中间不再保留任何真实副本。
  - local 源：软链直接指向源路径。
  - git 源：软链指向 `~/Library/Application Support/nyleen/git/<owner>/<repo>/`。
- 「安装 skill 到 target」时按需为该 target 创建软链，不再区分全局 target 与 project target 的写入方式——**全部统一为软链**。
- 据此修正历史文档错误：旧 README 中「project target → 真 copy」的描述与实现不符，现统一更正为「统一软链」。

### 3. git 源采用持久 clone 路径，按 `<owner>/<repo>` 索引

- git 源 clone 到 `~/Library/Application Support/nyleen/git/<owner>/<repo>/`（从 repo URL 解析，不再用 source_hash）。
- 首次使用时 `git clone`；已有 git 仓库只在上次成功更新满 24 小时、Git ref 改变、刷新记录缺失或系统时间回退时，才走增量更新（`git fetch` + `git checkout <ref>` + `git pull --ff-only`）。用户可在 Git 来源行手动强制拉取。
- 同一 repo 的不同分支共用一个 cache 目录（用户选择共用目录方案）。
- clone 落点是持久路径，软链直接指向它，路径可预测、可追溯。
- 删除 source 时，若该 repo 没有被其他 source 引用，则删除 `git/<owner>/<repo>/` 目录。

### 4. 同步语义与实际状态

- 每次同步某 source：发现当前 skills，先清理用户勾选的 target 中解析后指向该 source 根路径的旧软链接，再按本次勾选项创建目录软链接。
- source 的导入和编辑只更新 `config.yaml`，不会自动创建、更新或撤回 target 中的软链接。
- skill 的安装状态以 target 目录内的实际软链接为准，不记录在 `config.yaml`。

### 5. 同步/移除/删除的清理范围

- **同步（`sync_source_to_targets`）**：只对**用户勾选的 targets**操作——先移除这些 target 里指向该 source 的旧软链，再按勾选的 skills 重建。未勾选的 target 不受影响。
- **移除同步（`remove_source_sync`）**：清空**勾选的 targets**里该 source 安装的全部软链，不重建。不需要选 skills，只选 targets。
- **删除 source（`delete_skill_source` / `delete_skill_sources`）**：
  - config.yaml 中该 source 的条目。
  - 所有 target 中解析后指向该 source 根路径的软链接。
  - 该 source 对应的 git cache（仅当该 repo 没有被其他 source 引用时才删）。

**不**触碰 target 中的真实目录、普通文件，以及不指向当前 source 根路径的软链接。

## 理由

- **消除冗余拷贝**：直接软链意味着同一份源内容在磁盘上只存在一份（local）或一份 clone（git），不再为每个 workspace、每次同步重复 copy，显著节省空间，也避免副本与源漂移。
- **降低复杂度**：去掉「副本」这一中间层后，同步、删除、inspect 不再需要围绕副本目录做空壳清理与路径维护，链路从两段式收敛为一段式。
- **git cache 可控**：`git/<owner>/<repo>/` 作为唯一持久落点，让 clone 结果可预测、可在删 source 时确定性地回收，消除旧实现中 clone 散落、难以追溯的问题。
- **统一写入语义**：全局 target 与 project target 不再用两套写入方式，减少分支与潜在 bug（包括旧 README 误描述所反映的认知偏差）。
- **配置收口**：单一全局配置让 sources/targets/projects/mcps 的定义集中可维护，不再需要为「选哪个 workspace」付出额外的 UI 与状态成本。
- **状态可验证**：直接读取 target 中的软链接即可得到当前分发状态；当 source 根路径重叠时，文件系统无法证明最初由哪个 source 创建，因此界面只能表述为“检测到指向该来源路径”。

## 影响

### 配置文件迁移

- 旧：workspace 目录内的 `nyleen.yaml`（带 `workspace_name`）。
- 新：`~/Library/Application Support/nyleen/config.yaml`（无 `workspace_name`，不保存 skill 安装条目）。
- 用户需将既有配置迁移到新路径；旧字段在新格式下不再被读取。

### 路径变化

- 配置文件：`~/Library/Application Support/nyleen/config.yaml`。
- git cache：`~/Library/Application Support/nyleen/git/<owner>/<repo>/`。
- 不再存在 `<workspace>/skills/<hash>/<id>/` 这类副本目录。

### 软链机制

- local 源软链指向源路径；git 源软链指向 `git/<owner>/<repo>/`。
- 软链随源头目录自然可见：源头被同步流程重建时，软链下的 skill 自动跟上。
- 删除 source 时该 source 的软链会被一并删除。

### git cache 策略

- 每个 git source 在 `git/<owner>/<repo>/` 拥有持久 clone（同一 repo 的不同分支共用）。
- 首次 clone 后，常规发现和打开界面复用本地 cache；仅在 24 小时刷新条件满足时才增量 pull。Git 来源行可主动强制拉取，绕过间隔。
- 删除 source 时，仅当该 repo 没有被其他 source 引用时才删除其 cache。

### 删除语义

- manager 仅删除解析后指向当前 source 根路径的软链接，清理范围为「清 config + 删匹配软链 + 删 git cache」。
- 同步和移除同步操作严格限定在用户勾选的 targets 范围内；只有删除 source 才会扫所有 target 清软链。
- 「单删 skill」功能依然不存在（后端命令、前端按钮、批量选择入口均已移除）。

## 后续工作

- **project manifest 机制（Phase 6 预留）**：当前 `projects` 的定义已进入 config，但 project target 如何与具体项目目录、项目级 manifest 联动尚未最终定型。后续会在 Phase 6 阶段补一份独立的 ADR，明确 project target 的安装状态判定、软链落点以及与全局 target 的差异边界。
- **迁移与文档对齐**：在落地过程中同步更新 README、前后端 contract 对照表（`aidocs/decisions/2026-05-22-frontend-backend-contracts.md`）以及测试，确保不再残留 `workspace_name`、`workspace_path`、`missing` 等旧字段，以及「真 copy」相关描述。
