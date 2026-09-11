# Reins

本地桌面工具，提供两组能力：

- 浏览 Codex、Claude Code、OpenCode、Pi 的历史会话，并在不同工具之间导入
- 通过全局配置文件 `config.yaml`，把 Skills 和 MCP 配置分发到多个 agent target

## 功能概览

### 历史会话

- 自动发现本机的 Codex、Claude Code、OpenCode、Pi 会话
- 按来源浏览会话列表、消息、事件和工具调用
- 支持搜索、排序和增量加载
- 支持跨工具导入历史会话
- OpenCode root session 会自动聚合子会话；Pi 按工作目录读取 `~/.pi/agent/sessions/` 下的 JSONL
- Pi 的 subagent 每次运行在时间线内各占一行入口（代理名、状态、任务标题），并行派出的多个子代理不合并；点击在大弹窗中查看该次运行的任务、用量、失败原因与完整过程
- 消息页为文档流布局：用户输入右侧气泡，助手文本靠左，工具调用按 toolCallId 配对合并为一行可展开摘要；时间线始终展示完整消息流

### 配置与分发

- 维护全局单一配置 `config.yaml`
- 管理多个 target，例如 `codex`、`claude`、`opencode`、`zcode`
- 从本地目录或 Git 仓库发现、导入和同步 Skills
- 在 skills tab 顶部点击 `新增来源` 导入 source；每个 source 行内提供 `编辑` / `删除` / `同步` / `移除同步`
- 预览并写入 MCP 配置到各 target 的配置文件
- 在界面里检查 MCP 在各 target 上的安装状态

`config.yaml` 与 git cache 都存放在 Reins 数据目录下，按各系统标准配置目录 + `reins` 解析：macOS 为 `~/Library/Application Support/reins`，Linux 为 `~/.config/reins`（遵循 `XDG_CONFIG_HOME`），Windows 为 `%APPDATA%\reins`。下文用 `<reins 目录>` 指代该目录。

### Skills 导入项管理

skills tab 直接列出全部已配置来源。顶部 `新增来源` 用于导入本地目录或 Git 仓库；本地来源行提供 `编辑` / `删除` / `同步` / `移除同步`，Git 来源额外提供 `强制拉取`。

- `编辑`：修改 Git ref 以及 include name/path patterns，只更新 `config.yaml`，不会同步或撤回 target 中已有的软链接。
- `同步`：打开同步弹窗，选择 skills 与 targets；确认后调用 `sync_source_to_targets`。
- 同步目标列表默认收起每个 target 的关联 skills；展开后，当前 source 命中的链接会用主色高亮，源已删除或已排除的链接标记为待清理，非受管链接只显示右侧状态标签。
- `移除同步`：选择 targets，清理该 source 已分发到这些 targets 的软链。
- `强制拉取`：仅 Git 来源可用，忽略自动更新间隔，立即执行 fetch、checkout 和 fast-forward pull；成功后刷新界面状态。Git 来源行会展示该 cache 最近一次成功拉取时间；尚未建立 cache 时显示“尚未拉取”。
- `删除`：弹出二次确认；确认后调用 `delete_skill_source`，删除该 source 条目、解析后指向该 source 根路径的软链，并在没有其他 source 引用同一 repo 时清掉 git cache（`<reins 目录>/git/<owner>/<repo>/`）。

### Manager 管理的软链接清理边界

`config.yaml` 不保存 skill 安装注册表；target 目录中的实际软链接是分发状态的依据。manager 只会删除解析后指向某个 source 根目录的目录软链接，不会删除真实目录、普通文件或指向其它位置的软链接。

- **写入（统一软链）**：
  - 同步 source 时不再做任何真实 copy，所有分发都通过软链完成。
  - local 源 → 软链直接指向源路径。
  - git 源 → clone 到 `<reins 目录>/git/<owner>/<repo>/`（从 repo URL 解析）。首次使用才 clone；已有 cache 默认仅在上次成功更新满 24 小时、Git ref 改变、刷新记录缺失或系统时间回退时，执行增量更新（`git fetch` + `checkout <ref>` + `git pull --ff-only`）。用户可在 Git 来源行点击 `强制拉取` 立即更新。软链指向该 clone 路径。
  - 「安装 skill 到 target」时按需为该 target 创建软链，不再区分全局 / project target 的写入方式。
- **导入和编辑 source**：只写入 `config.yaml`；不会创建、更新或撤回 target 中已有的软链接。
- **同步（`sync_source_to_targets`）**：只影响用户勾选的 targets——先移除这些 target 里指向该 source 的旧软链，再按勾选的 skills 重建。未勾选的 target 不受影响。
- **移除同步（`remove_source_sync`）**：清空勾选的 targets 里该 source 安装的全部软链。在同步弹窗里点「移除同步」触发，只需选 targets，不需要选 skills。
- **删除 source（`delete_skill_source` / 批量删除）会清理的范围**：
  - `config.yaml` 中该 source 的条目
  - 所有 target 中解析后指向该 source 根路径的软链接（指向源或 git clone）
  - 该 source 对应的 git cache（`<reins 目录>/git/<owner>/<repo>/`，仅当该 repo 没被其他 source 引用时才删）
- **不由 manager 删除的内容**：
  - target 里的真实目录、普通文件，以及不指向当前 source 根目录的软链接 → **由用户自行管理**
  - 「单删 skill」功能已移除（后端命令、前端按钮、批量选择 + 删除入口全部清除）
- **如果被应用的技能不再匹配 include name/path patterns**：编辑 source 后不会自动撤回；用户对该 target 再次同步时，旧链接会被移除，并按本次勾选的 skills 重建。

软链天然跟随其指向的源头目录：源头被同步/导入流程重建时，软链下的 skill 自然可见；删 source 时，属于该 source 的软链会被一并删除。

## 导入说明

- 同程序导入选项在界面中隐藏
- 导入只会创建新会话，不会覆盖原始数据
- 导入到 Codex 时写出 transcript JSONL
- 导入到 Claude Code 时写入项目目录 JSONL
- 导入到 OpenCode 时通过官方 CLI 完成
- 导入到 Pi 时按 cwd 编码目录写入 Pi v3 session JSONL

## 配置文件

配置采用全局单一文件：`<reins 目录>/config.yaml`（目录在各系统下的具体位置见「配置与分发」）。文件由程序维护，用户也可以手动编辑，承载 sources / targets / projects / mcps 的定义；skill 的实际分发状态由各 target 目录中的软链接表示。

> 该文件取代了旧版本中的 workspace 级 `nyleen.yaml`（历史命名）。旧字段 `workspace_name` 已废弃，skill 条目中的 `workspace_path` / `missing` 字段也一并移除。

最小示例：

```yaml
targets:
  codex:
    enabled: true
    skill_dir: ~/.agents/skills
    mcp:
      enabled: true
      config_path: ~/.codex/config.toml
      config_prefix: mcp_servers
      config_type: common

  claude:
    enabled: true
    skill_dir: ~/.claude/skills
    mcp:
      enabled: true
      config_path: ~/.claude.json
      config_prefix: mcpServers
      config_type: common

  opencode:
    enabled: true
    skill_dir: ~/.config/opencode/skills
    mcp:
      enabled: true
      config_path: ~/.config/opencode/opencode.json
      config_prefix: mcp
      config_type: opencode

  zcode:
    enabled: true
    skill_dir: ~/.zcode/skills
    mcp:
      enabled: true
      config_path: ~/.zcode/cli/config.json
      config_prefix: mcp.servers
      config_type: common

skill_sources: []
projects:
  my-app:
    path: ~/code/my-app
    agents:
      opencode:
        enabled: true
mcps: []
```

OpenCode 的全局 target 写入 `~/.config/opencode/skills/<name>/SKILL.md` 与 `~/.config/opencode/opencode.json`；项目 target 写入 `<project>/.opencode/skills/<name>/SKILL.md` 与 `<project>/opencode.json`。

ZCode 的全局 target 写入 `~/.zcode/skills/<name>/SKILL.md` 与 `~/.zcode/cli/config.json` 的 `mcp.servers`；项目 target 写入 `<project>/.zcode/skills/<name>/SKILL.md` 与 `<project>/.zcode/config.json` 的 `mcp.servers`。

## 开发

要求：

- Node.js
- pnpm
- Rust toolchain
- Tauri 桌面依赖

常用命令：

```bash
pnpm install
pnpm check
pnpm tauri dev
```

运行 Rust 测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

运行前端测试：

```bash
pnpm test
```

端到端测试依赖真实环境和对应 CLI：

```bash
OPENCODE_SESSION_ID=ses_xxx cargo test --manifest-path src-tauri/Cargo.toml imports_real_opencode_session_into_codex_and_resumes -- --ignored --nocapture
```

## 构建

```bash
pnpm tauri build
```

## 运行约定

- 顶部模式切换分为“历史会话”和“配置与分发”
- 历史会话详情页只展示跨程序导入目标
- 会话列表滚动到底部会自动加载更多
- OpenCode 会聚合 root session 与子会话

## 图标

应用图标使用 AI 生成。
