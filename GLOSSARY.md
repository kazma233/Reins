# reins

reins 是管理多个 AI CLI 编码工具的桌面应用：浏览历史会话、统计 token 用量、配置聚合平台模型、向各工具分发 MCP 与 skills。术语按域分组，中文名为准。

## 语言

### 通用

**工具清单（agents / AgentSpec）**：
reins 管理的全部 AI 编码工具的静态描述表：规范键、产品名、跨域 id 映射、全局与项目布局；是唯一的单一来源，各域的 defaults 表、模板与文案从中派生。
_Avoid_: agent 列表、target 表（指派生出的副本）

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

**读取器引擎（reader engine）**：
family 形态读取器共享的机械底座：缓存生命周期（目录索引、双半时间线、三级摘要）、失效判定、marker 注入、排序分页、clear 编排；读取器只声明真差异，典型来源零覆写。
_Avoid_: harness（域内该词专指 DSH/DeepSeek Harness 这类被管理的 agent 运行时）、runtime

**会话目录（catalog）**：
一个来源的会话文件枚举结果与缓存，列表分页的数据源。
_Avoid_: 索引（易与会话族索引混淆）

**会话族（family）**：
同一逻辑会话的全部转录文件（root、resume 段、子代理线程），列表与时间线以族为单位聚合。
_Avoid_: 会话组

**孤儿策略（orphan policy）**：
父链分组的族根上溯在父节点缺失时的处置语义：认领缺失父 id 自成一家（OpenCode/ZCode）、保留最后已知 id 挂靠存活祖先（Codex）；DSH 走 catalog 声明的边集、链断即隐藏，不经共享部件。由 family_index 的 `OrphanPolicy` 显式声明，读取器不得静默统一。
_Avoid_: 兜底分组

**会话身份（session identity）**：
跨层寻址一个会话的唯一二元组（来源, source_session_id）；transcript_path 只是展示字段，不参与寻址、缓存键或前端主键。
_Avoid_: 路径主键、transcript key

**删除计划（delete plan）**：
一次会话删除的预演：后端按来源给出的动作清单（删文件/删目录/官方命令/sql 清理）与说明文案；预览与确认框只渲染计划，删除语义的唯一真相在后端删除引擎。
_Avoid_: 命令预览（指渲染层格式化后的字符串）

### 工作区（workspace 域）

**目标（target）**：
接受 skills 与 MCP 分发的一份工具配置，分全局与项目两级。
_Avoid_: agent（太泛）、容器

**环境重定向（env redirect）**：
工具官方支持的全局目录重定位（如 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`GROK_HOME`、`PI_CODING_AGENT_DIR`、`DSH_HOME`）。认哪个变量、默认目录是什么由 `agents.rs` 清单的 `GlobalRoot` 持有，解析统一走 `support::fs::env_home_dir`；不随重定向搬移的落点在清单里以 `~/` 前缀显式标注。域内自己再拼一份 `~/.codex` 之类的路径视为重复定义。
_Avoid_: 路径助手（指各域自带的路径拼接函数）

**安装面（install surface）**：
「把一个 skill 装进一个 target」的物理语义：目标是否参与分发、目录链接落在哪、落点当前是什么（未安装 / 已指向本 skill / 指向同来源别的 skill / 指向别处 / 真实文件目录）、装与卸的结果。安装、冲突预演、链接枚举、按来源卸载四条路径都经 `workspace/skill_install.rs`，判定只此一份。
_Avoid_: 链接工具函数（指各调用方自带的路径比较与 skip 判断）

**启停（enable/disable）**：
已废弃的概念。target 与 MCP 条目都没有开关：配置在盘上就是生效的，不需要分发就不写进该 target 的配置；旧 config.yaml 里的 `enabled` 键被忽略且不再写回。
_Avoid_: 停用目标、禁用 MCP（指旧版本里的开关语义）

**目标地址（target address）**：
一个 target 的标识，全局 target 就是它的 id，项目级是 `项目:agent`。两段只在寻址层被拆解或拼装，其余地方把地址当不透明关联键使用。
_Avoid_: 复合 id（只在讨论格式本身时使用）

**跳过（skip）**：
「这个目标地址不参与分发」的领域判定，只有一处（寻址层）。它回答原因（地址不存在 / skill_dir 整层是链接），调用方只决定表现方式：报错、提示或静默。
_Avoid_: 禁用（已废弃的启停概念）、忽略

**技能源（skill source）**：
workspace 管理的 skill 来源（git 或本地目录），负责发现、同步、导入与分发。
_Avoid_: skill 仓库

**MCP 格式 writer（MCP format writer）**：
一种目标工具 MCP 配置格式的完整知识（条目形态、支持的文件格式、前缀约束、读写与预览）；mcps 域的编排只经唯一分发点调用 writer，不再按工具类型写特例。
_Avoid_: config type 分发（指旧的散落 match 形态）

### 模型配置（providers 域）

**聚合平台（provider）**：
除 OpenAI、Anthropic 官方外的一切模型平台与自建网关，由 reins 管理端点、协议、模型与 API Key。
_Avoid_: 供应商、平台（单独使用太泛）

**方言（dialect）**：
一个归一化枚举（协议、思考档位）在某工具配置文件里的实际拼写知识：正向写入、反向识别（含只读别名与有损折叠）由同一张表派生，各适配器经 `AppAdapter` 声明（Pi 与 dsh 共用 pi-ai 方言），roundtrip 测试守卫正反互逆。
_Avoid_: 映射函数（指散落在适配器里的正反函数对）
