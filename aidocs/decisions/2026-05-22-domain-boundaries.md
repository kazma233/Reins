# 业务域命名与边界

- 日期：2026-05-22
- 状态：生效中

## 命名约定

1. 前端业务域统一使用复数目录：`src/features/sessions/`、`src/features/workspace/`。
2. Rust 业务域统一使用单数模块：`src-tauri/src/session/`、`src-tauri/src/workspace/`。
3. UI 文案继续使用“历史会话”和“配置与分发”，不强行暴露内部 `workspace` 命名。
4. `manager` 只允许出现在历史兼容数据名或稳定文件名里，不再作为代码边界命名。

## 前端边界

### `sessions`

1. 负责 sources 探测、会话列表、overview、timeline、导入、删除。
2. 只依赖 `shared/*` 和 `app/*` 的稳定入口。
3. 不依赖 `features/workspace/*`。

### `workspace`

1. 负责 workspace 选择、`nyleen.yaml` 读取与修改、targets/skills/mcps 的管理与分发。
2. 只依赖 `shared/*` 和 `app/*` 的稳定入口。
3. 不依赖 `features/sessions/*`。

### `shared`

1. 只放稳定 UI、基础格式化、无业务语义工具。
2. 不承载 session/workspace 的业务类型、store、api、命令名。

## Rust 边界

### `session`

1. 负责 Codex / Claude Code / OpenCode 的会话发现、解析、overview、timeline、导入、删除。
2. `model.rs` 只定义 session DTO 和内部 timeline/cache 数据。
3. `commands.rs` 只负责 Tauri command 入参与调度。

### `workspace`

1. 负责 workspace preferences、`nyleen.yaml` 解析校验、inspection、targets、skills、mcps。
2. `types.rs` 只定义 workspace 域 DTO、view model、mutation 输入输出。
3. `commands.rs` 只负责 Tauri command 入参与调度。

### `state`

1. 只放运行时缓存状态：session index、skill discovery。
2. 不承载业务 command、文件格式解析或 DTO 拼装逻辑。

### `support`

1. 只放无业务语义工具：fs、paging、time。
2. 不接收 session/workspace DTO，不反向依赖业务域。

## workspace 子边界

### `targets`

1. 表示目标 agent 配置。
2. 负责 skill 目录、MCP 配置入口、启停状态。
3. 不负责 skill 源同步或 MCP 具体定义内容。

### `skills`

1. 表示 workspace 管理的 skill 来源与导入结果。
2. 负责 git/local discovery、同步、导入到 workspace、分发到 target。
3. 不负责 target 的基础定义，也不负责 MCP 配置格式。

### `mcps`

1. 表示 workspace 管理的 MCP server 定义。
2. 负责 MCP CRUD、预览写入结果、应用到 target、从 target 移除。
3. 不负责 skill 源发现，也不负责 session 数据。

## 依赖红线

1. `sessions` 和 `workspace` 之间禁止直接互相依赖。
2. `shared` 禁止依赖 `features/*`。
3. `support` 禁止依赖 `session` 和 `workspace` DTO。
4. 新增代码如果需要跨域复用，先判断它是否真的是稳定共享能力，否则仍留在各自业务域。
