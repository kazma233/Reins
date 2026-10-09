# Reins

本地桌面工具，提供三组能力：

- 浏览 Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode 的历史会话
- 通过全局配置文件 `config.yaml`，把 Skills 和 MCP 配置分发到多个 agent target
- 管理聚合提供商（OpenRouter、自建网关等）的模型配置，一键应用到 Codex、Claude Code、OpenCode、Pi、Grok Build

## 功能概览

### 历史会话

- 自动发现本机的 Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode 会话（ZCode 读取 `~/.zcode/cli/db/db.sqlite`）
- 按来源浏览会话列表、消息、事件和工具调用；「全部」视图跨来源合并浏览与搜索
- 支持搜索（标题 / Session ID）和排序
- 会话列表滚动到底部自动加载更多
- 会话列表与详情页展示 token 消耗（输入 / 输出 / 缓存及命中率）
- OpenCode、ZCode 会话自动聚合子会话；Codex / Claude Code 会话自动聚合续跑、fork 与 subagent 线程
- Grok Build 会话自动折叠已确认的 subagent 子会话，用量统计含子代理汇总
- 可单独查看 Pi subagent 每次运行的任务、消耗、失败原因与完整过程
- 消息页完整展示消息流，工具调用合并为可展开查看的摘要
- 会话可删除（ZCode 暂不支持），删除前有二次确认与等价命令预览

### 配置与分发

- 集中管理 Skills 和 MCP 配置
- 管理多个 target，内置 `codex` / `claude` / `opencode` / `zcode` / `pi` / `grokbuild` 六种 preset，另支持项目级 target
- 从本地目录或 Git 仓库导入 Skills（Git 支持批量导入），并同步到选定的 target
- 预览并写入 MCP 配置，检查其在各 target 上的安装状态；Pi（≥0.99）的 MCP 写 `~/.pi/agent/mcp.json` 顶层 `mcpServers`，legacy SSE transport 不受支持（分发界面置灰）

### 模型配置

- 集中管理聚合提供商的端点、协议（OpenAI Responses / OpenAI Chat Completions / Anthropic Messages）、模型目录与 API Key；官方 OpenAI / Anthropic 不纳管
- 提供商元数据与 API Key 存在同一份 `<reins 目录>/providers.yaml`，Key 为明文（不加密），文件即凭据边界
- 模型支持手工新增、从提供商 `/models` 接口拉取、从 models.dev 预填元数据（本地缓存 24 小时）
- 一键应用到 Codex、Claude Code、OpenCode（v2）、Pi、Grok Build 的全局配置文件：应用前可预览变更 diff（密钥脱敏），支持指定默认模型与思考等级
- 应用细节对齐各工具官方契约：Codex 同步生成 `model_catalog_json` 模型目录，已选模型出现在 Codex 自己的模型选择器；Claude Code 认证写 `ANTHROPIC_AUTH_TOKEN`（Bearer 头）；OpenCode 同时读写 `opencode.json` 与 `opencode.jsonc`，被覆盖文件里的条目标注不生效
- 支持 `CODEX_HOME`、`PI_CODING_AGENT_DIR`、`GROK_HOME`、`CLAUDE_CONFIG_DIR` 环境变量重定向各工具的配置目录（作用于模型配置写入；历史会话发现固定读各工具默认 home，Pi 与 Grok Build 例外，跟随各自环境变量；Pi 的 workspace preset 路径在首次添加 target 或首次生成 `config.yaml` 时按 `PI_CODING_AGENT_DIR` 解析固化，之后以 `config.yaml` 里写入的路径为准）
- 反读工具配置，识别「已应用 / 配置有偏差 / 外部配置」；已应用或配置有偏差的提供商不再重复出现在「应用提供商」候选里；已应用条目可重新应用切换默认模型、可移除；外部条目可按条目删除（带强警告确认，Claude Code 不支持）；Reins 写入均用 `reins-` 前缀条目，与用户手工配置隔离

### Skills 导入项管理

`config.yaml` 与 git cache 都存放在 Reins 数据目录下，按各系统标准配置目录 + `reins` 解析：macOS 为 `~/Library/Application Support/reins`，Linux 为 `~/.config/reins`（遵循 `XDG_CONFIG_HOME`），Windows 为 `%APPDATA%\reins`。下文用 `<reins 目录>` 指代该目录。

skills tab 直接列出全部已配置来源。顶部 `新增来源` 用于导入本地目录或 Git 仓库；来源行提供 `同步` / `编辑` / `删除`，其余操作都在同步弹窗内完成。

- `编辑`：修改 Git ref 以及 include name/path patterns，只更新 `config.yaml`，不会同步或撤回 target 中已有的软链接。
- `同步`：打开同步弹窗，选择 skills 与 targets；目标处存在同名冲突时先预览冲突列表，确认覆盖后才执行 `sync_source_to_targets`。
- 同步目标列表默认收起每个 target 的关联 skills；展开后，当前 source 命中的链接会用主色高亮，源已删除或已排除的链接标记为待清理，非受管链接可逐条移除软链。
- `移除同步`：同步弹窗底部入口，选择 targets，清理该 source 已分发到这些 targets 的软链。
- `强制拉取`：同步弹窗 Skills 列头入口，仅 Git 来源可用，忽略自动更新间隔，立即执行 fetch、checkout 和 fast-forward pull；成功后刷新界面状态。Git 来源行会展示该 cache 最近一次成功拉取时间；尚未建立 cache 时显示“尚未拉取”。
- `删除`：弹出二次确认；确认后调用 `delete_skill_source`，删除该 source 条目、解析后指向该 source 根路径的软链，并在没有其他 source 引用同一 repo 时清掉 git cache（`<reins 目录>/git/<owner>/<repo>/`）。

### Manager 管理的软链接清理边界

`config.yaml` 不保存 skill 安装注册表；target 目录中的实际软链接是分发状态的依据。manager 只会删除解析后指向某个 source 根目录的目录软链接，不会删除真实目录、普通文件或指向其它位置的软链接。

- **写入（统一软链）**：
  - 同步 source 时不做任何真实 copy，所有分发都通过软链完成；创建软链的唯一路径是 `sync_source_to_targets`。
  - local 源 → 软链直接指向源路径。
  - git 源 → clone 到 `<reins 目录>/git/<owner>/<repo>/`（从 repo URL 解析）。首次使用才 clone；已有 cache 默认仅在上次成功更新满 24 小时、Git ref 改变、刷新记录缺失或系统时间回退时，执行增量更新（`git fetch` + `checkout <ref>` + `git pull --ff-only`）。用户可在同步弹窗点「强制拉取」立即更新。软链指向该 clone 路径。
- **导入和编辑 source**：只写入 `config.yaml`；不会创建、更新或撤回 target 中已有的软链接。
- **同步（`sync_source_to_targets`）**：只影响用户勾选的 targets——先移除这些 target 里指向该 source 的旧软链，再按勾选的 skills 重建。未勾选的 target 不受影响。
- **移除同步（`remove_source_sync`）**：清空勾选的 targets 里该 source 安装的全部软链。在同步弹窗里点「移除同步」触发，只需选 targets，不需要选 skills。
- **删除 source（`delete_skill_source`）会清理的范围**：
  - `config.yaml` 中该 source 的条目
  - 所有 target 中解析后指向该 source 根路径的软链接（指向源或 git clone）
  - 该 source 对应的 git cache（`<reins 目录>/git/<owner>/<repo>/`，仅当该 repo 没被其他 source 引用时才删）
- **不由 manager 删除的内容**：
  - target 里的真实目录、普通文件，以及不指向当前 source 根目录的软链接 → **由用户自行管理**（同步覆盖在用户确认后例外）
  - 旧的「单删 skill」入口已移除；保留的窄能力是同步弹窗里对**非受管链接**的逐条移除（`remove_target_skill_link`，仅限 target skills 目录内的目录软链，拒绝真实目录与文件）
- **如果被应用的技能不再匹配 include name/path patterns**：编辑 source 后不会自动撤回；用户对该 target 再次同步时，旧链接会被移除，并按本次勾选的 skills 重建。

软链天然跟随其指向的源头目录：源头被同步/导入流程重建时，软链下的 skill 自然可见；删 source 时，属于该 source 的软链会被一并删除。

## 参考项目

同类开源工具，Reins 的模型配置模块在写入口径上与它们互为对照：

- [cc-switch](https://github.com/farion1231/cc-switch)：Tauri 2 桌面应用，一站管理 Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build、OpenCode、Pi 等十个工具的 Provider 切换。内置 90+ 提供商预设与本地协议路由（Anthropic / OpenAI / Gemini 格式互转、自动故障转移），MCP、Skills 与 Prompts 集中分发，并带用量与配额展示。
- [magpie](https://github.com/yetone/magpie)：菜单栏应用（Go + Wails），让所有 agent 统一指向一个本地网关（`127.0.0.1:3425`，同时说 OpenAI Chat / Responses、Anthropic Messages 与 Gemini API），由网关做协议翻译与转发；已登录的订阅（Claude Code、Codex、Copilot 等）也能作为 Provider 共享给其它工具；模型列表来自厂商实时接口与 models.dev，配置编辑保留注释与键序。

## 开发与构建

环境准备、本地开发与打包遵循 Tauri v2 官方文档：[https://v2.tauri.app/](https://v2.tauri.app/)

## 图标

应用图标使用 AI 生成。
