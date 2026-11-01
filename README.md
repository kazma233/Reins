# Reins

本地桌面工具，提供三组能力：

- 浏览 Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode、DSH（DeepSeek Harness）的历史会话
- 通过全局配置文件 `config.yaml`，把 Skills 和 MCP 配置分发到多个 agent target
- 管理聚合提供商（OpenRouter、自建网关等）的模型配置，一键应用到 Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode、DSH

## 安装

到 [Releases](https://github.com/kazma233/Reins/releases) 下载对应平台的安装包；暂无对应平台的包时，按[开发与构建](#开发与构建)准备环境后执行 `pnpm tauri build` 自行构建。

## 数据目录

`config.yaml`、`providers.yaml` 与 git cache 都存放在 Reins 数据目录下，按各系统标准配置目录 + `reins` 解析：macOS 为 `~/Library/Application Support/reins`，Linux 为 `~/.config/reins`（遵循 `XDG_CONFIG_HOME`），Windows 为 `%APPDATA%\reins`。下文用 `<reins 目录>` 指代该目录。

## 功能概览

### 历史会话

- 自动发现本机的 Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode、DSH 会话（ZCode 读取 `~/.zcode/cli/db/db.sqlite`）
- 按来源浏览会话列表、消息、事件和工具调用；「全部」视图跨来源合并浏览与搜索
- 会话列表右上角可跳到用量统计页，统计页顶部有「返回会话列表」
- 「全部」视图下每条会话标出所属工具（Codex / Claude Code / OpenCode / Pi / Grok Build / ZCode / DSH），单一来源时不重复显示
- 会话列表整屏铺开、标题整行换行完整显示（不再按字符数截断）；点开某条会话进入独立页面查看整个会话，返回入口固定在页面顶部
- 支持搜索（标题 / Session ID）和排序
- 会话列表滚动到底部自动加载更多
- 会话列表与详情页展示 token 消耗（列表：总消耗 + 缓存命中率；详情展开输入 / 输出 / 缓存读写的精确值）
- 会话列表角标与详情页展示会话用过的模型与供应商（多模型显示 `主模型 +N`）；消息页在每次输入与输出旁标注该轮使用的模型
- 消息页提供「只看对话」勾选框：把所有思考与工具过程行收成一行摘要（如「过程 12 项」，单条过程也收），时间线上只剩对话正文，点开摘要仍是原有过程行、可逐条展开；筛选（搜索）时不做分组，命中项直接可见
- OpenCode、ZCode 会话自动聚合子会话；Codex / Claude Code 会话自动聚合续跑、fork 与 subagent 线程
- Grok Build 会话自动折叠已确认的 subagent 子会话，用量统计含子代理汇总
- 可单独查看 Pi subagent 每次运行的任务、消耗、失败原因与完整过程
- 消息页完整展示消息流，工具调用合并为可展开查看的摘要
- 会话可删除（ZCode 暂不支持），删除前有二次确认与等价命令预览

### 配置与分发

- 集中管理 Skills 和 MCP 配置
- 管理多个 target，内置 `codex` / `claude` / `opencode` / `zcode` / `pi` / `grokbuild` / `dsh` 七种 preset，另支持项目级 target
- 从本地目录或 Git 仓库导入 Skills（Git 支持批量导入），并同步到选定的 target
- 预览并写入 MCP 配置，检查其在各 target 上的安装状态；Pi（≥0.99）的 MCP 写 `~/.pi/agent/mcp.json` 顶层 `mcpServers`，legacy SSE transport 不受支持（分发界面置灰）

### 模型配置

- 集中管理聚合提供商的端点、协议（OpenAI Responses / OpenAI Chat Completions / Anthropic Messages）、模型目录与 API Key；官方 OpenAI / Anthropic 不纳管
- 提供商元数据与 API Key 存在同一份 `<reins 目录>/providers.yaml`，Key 为明文（不加密），文件即凭据边界
- 模型支持手工新增、从提供商 `/models` 接口拉取、从 models.dev 预填元数据（本地缓存 24 小时）
- 一键应用到 Codex、Claude Code、OpenCode（v2）、Pi、Grok Build、ZCode、DSH 的全局配置文件：应用前可预览变更 diff（密钥脱敏），支持指定默认模型与思考等级
- 应用细节对齐各工具官方契约：Codex 同步生成 `model_catalog_json` 模型目录，已选模型出现在 Codex 自己的模型选择器；Claude Code 认证写 `ANTHROPIC_AUTH_TOKEN`（Bearer 头）；OpenCode 同时读写 `opencode.json` 与 `opencode.jsonc`，被覆盖文件里的条目标注不生效
- 支持 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`GROK_HOME`、`PI_CODING_AGENT_DIR`、`DSH_HOME` 环境变量重定向各工具的根目录：模型配置写入与历史会话发现共用同一条解析（重定向优先、HOME 兜底），各处看到同一份数据；Pi 的会话根另有官方 sessionDir 链等特有契约，在重定向之上自行解析。Pi 的 workspace preset 路径在首次添加 target 或首次生成 `config.yaml` 时按 `PI_CODING_AGENT_DIR` 解析固化，之后以 `config.yaml` 里写入的路径为准
- 反读工具配置，识别「已应用 / 配置有偏差 / 外部配置」；已应用或配置有偏差的提供商不再重复出现在「应用提供商」候选里；已应用条目可重新应用切换默认模型、可移除；外部条目可按条目删除（带强警告确认，Claude Code 不支持）；Reins 写入均用 `reins-` 前缀条目，与用户手工配置隔离

### Skills 与软链分发

Skills 来源支持本地目录与 Git 仓库（Git 支持 ref 与 include name/path patterns、批量导入）。`编辑` 只改 `config.yaml`，不动已有软链；`同步` 打开弹窗选择 skills 与 targets，目标处同名冲突先预览确认；`移除同步` 清理该来源分发到所选 targets 的软链；`强制拉取` 忽略自动更新间隔立即更新 git cache；`删除` 移除来源条目、其软链与不再被其他来源引用的 git cache。

分发统一用目录软链，不做真实拷贝：local 源软链指向源路径，git 源 clone 到 `<reins 目录>/git/<owner>/<repo>/` 后软链指向 clone（默认 24 小时增量更新一次）。`config.yaml` 不保存安装注册表，target 目录里的软链即分发状态；Reins 只删除解析后指向来源根目录的软链，不碰真实目录、普通文件与指向其它位置的软链（同步弹窗内可逐条移除非受管软链）。技能不再匹配 patterns 时不自动撤回，再次同步时按本次勾选重建。

## 参考项目

同类开源工具，Reins 的模型配置模块在写入口径上与它们互为对照：

- [cc-switch](https://github.com/farion1231/cc-switch)：Tauri 2 桌面应用，一站管理 Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build、OpenCode、Pi 等十个工具的 Provider 切换。内置 90+ 提供商预设与本地协议路由（Anthropic / OpenAI / Gemini 格式互转、自动故障转移），MCP、Skills 与 Prompts 集中分发，并带用量与配额展示。
- [magpie](https://github.com/yetone/magpie)：菜单栏应用（Go + Wails），让所有 agent 统一指向一个本地网关（`127.0.0.1:3425`，同时说 OpenAI Chat / Responses、Anthropic Messages 与 Gemini API），由网关做协议翻译与转发；已登录的订阅（Claude Code、Codex、Copilot 等）也能作为 Provider 共享给其它工具；模型列表来自厂商实时接口与 models.dev，配置编辑保留注释与键序。

## 开发与构建

技术栈：Tauri v2（前端 Vue 3 + TypeScript + Vite，后端 Rust）。

环境要求：

- Node.js ≥ 20.19（Vite 8 的最低要求）与 pnpm
- Rust stable（≥ 1.85，edition 2024）
- 各平台系统依赖见 [Tauri v2 官方文档](https://v2.tauri.app/start/prerequisites/)

常用命令：

```bash
pnpm install                # 安装前端依赖
pnpm tauri dev              # 本地开发运行
pnpm check                  # 类型与编译检查（vue-tsc --noEmit + cargo check）
pnpm test                   # 前端单元测试（vitest）
pnpm build                  # 前端类型检查 + 构建
pnpm codegen                # 重新导出 ts-rs 生成的 TS 绑定（改 Rust 侧契约后运行）
cargo test --manifest-path src-tauri/Cargo.toml   # Rust 测试
```

Rust 测试请全量运行：部分测试夹具依赖 bin 产物，`cargo test --lib` 会因定位不到夹具而误报失败。

## 图标

应用图标使用 AI 生成。

## 许可证

[MIT](./LICENSE)
