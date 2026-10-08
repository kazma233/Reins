# reins

reins 是管理多个 AI CLI 编码工具的桌面应用：浏览历史会话、统计 token 用量、配置聚合平台模型、向各工具分发 MCP 与 skills。术语按域分组，中文名为准。

## 语言

### 会话（session 域）

**来源（Source / SourceApp）**：
reins 读取会话数据的一个 CLI 工具（Codex、Claude Code、OpenCode、Pi、Grok Build、ZCode、DSH）。
_Avoid_: 工具、app、backend

**来源注册表（source registry）**：
全部来源静态描述的单张表，一个来源一条；来源的探测、缓存清理、删除策略、用量采集都由它驱动，新增来源即新增一条。
_Avoid_: 分发表、来源清单（指迭代结果时另说）

**读取器（reader）**：
一个来源的会话数据解析实现，每个来源恰好一个。
_Avoid_: backend

**会话目录（catalog）**：
一个来源的会话文件枚举结果与缓存，列表分页的数据源。
_Avoid_: 索引（易与会话族索引混淆）

**会话族（family）**：
同一逻辑会话的全部转录文件（root、resume 段、子代理线程），列表与时间线以族为单位聚合。
_Avoid_: 会话组

### 工作区（workspace 域）

**目标（target）**：
接受 skills 与 MCP 分发的一份工具配置，分全局与项目两级。
_Avoid_: agent（太泛）、容器

**技能源（skill source）**：
workspace 管理的 skill 来源（git 或本地目录），负责发现、同步、导入与分发。
_Avoid_: skill 仓库

### 模型配置（providers 域）

**聚合平台（provider）**：
除 OpenAI、Anthropic 官方外的一切模型平台与自建网关，由 reins 管理端点、协议、模型与 API Key。
_Avoid_: 供应商、平台（单独使用太泛）
