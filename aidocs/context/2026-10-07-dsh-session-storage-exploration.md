# dsh (DeepSeek Harness) 探索与接入方案

- 日期:2026-10-07
- 性质:探索记录 + v1 接入方案。证据来源:官方源码 github.com/deepseek-ai/deepseek-harness(master)、官方文档站、Windows 本机实测(桌面版 DeepSeek Harness,`~/.dsh` 含 3 个已落盘会话,其中 1 个 261 事件的完整真实会话)。
- 已确认决策:v1 不支持会话删除;zstd 解压用 `ruzstd`(纯 Rust);范围含 sessions 来源、usage 统计、workspace target(skills+MCP)、providers 域,一次做完。

## 结论(TL;DR)

1. dsh 会话主存储是 `~/.dsh/sessions/<projectKey(cwd)>/<session-id>/session.v{N}.jsonl[.zstd]`,纯文件型,**无 SQLite**。
2. 转录是**多帧 zstd 拼接**的 JSONL(按写入批次追加独立 zstd 帧),读取器必须逐帧流式解压;任何"单帧 zstd 解压一次"的实现都会丢数据(Node 官方 API 就只解首帧,已实测踩坑)。
3. 事件模型是判别联合 `{type, seq, time(毫秒), data}`,官方 `docs/persistence-catalog.md` 有全量 schema(约 200 种 type),格式版本 v4 已定稿、v3 已发布,多代文件可共存(取最高代)。
4. token 用量挂在 `assistant/message.usage`,字段口径与 Reins `SessionTokenUsage` 一一对应(input=未命中输入),**无需换算**。
5. subagent 是独立 session 文件,父日志 `subagent/catalog` 事件携带 `childId` 可做 family 聚合;本机暂无 subagent 实测样本(见风险)。
6. MCP 配置不走 mcpServers JSON/TOML,而是 **Cordis patch YAML**(`~/.dsh/cordis.patch.yml`),需要全新的 McpConfigType 写入器。

## 一、dsh 概况与本机现状

- DeepSeek 官方 agent harness。形态:Electron 桌面版 + CLI(`dsh`,profile 体系:web/tui/headless 等,profile 是 Cordis 插件 bundle 的叠加栈)。
- LLM 层基于 pi 栈(配置键 `llm-pi-ai`),但**会话存储是 dsh 自有格式**,与 pi 会话格式无关。
- 本机:桌面版装于用户目录下的应用安装位置(Windows 为 `%LOCALAPPDATA%\Programs\DeepSeek Harness`),自带 CLI 于 `resources/runtime/cli/bin/dsh.cmd`,**dsh 不在 PATH**。
- CLI 能力:`dsh tui --resume <session>` 可恢复会话;**无官方单会话删除命令**;桌面版的"删除"语义是 workspace.json 里的 `archivedSessionIds`(archive)。→ v1 不做删除的直接依据。

## 二、目录布局(`~/.dsh/`)

| 路径 | 内容 | Reins 需要 |
| --- | --- | --- |
| `sessions/<projectKey>/<session-id>/session.v{N}.jsonl[.zstd]` | 会话转录(核心) | 是 |
| `storages/session_projcache/sessions/<session-id>.json` | 会话元数据侧车(title/cwd/token 总量/blank 标记,version:7 record) | v1 可不用(转录直读);了解即可 |
| `storages/workspace.json` | workspace 注册表 + archived/pinned sessionIds | 否 |
| `settings.yaml` | 全局配置,含模型路由(`llm-pi-ai`) | providers 域用 |
| `cordis.patch.yml`(全局)/ `profiles/<name>/cordis.patch.yml` | Cordis 插件 patch 层,**MCP 配置在这里** | workspace MCP 分发用 |
| `.credentials.yaml` | 凭据(**敏感边界,不读取**) | 否 |
| `skills/` | 用户级 skills(另有 `~/.agents/skills` 共享根) | workspace skills 分发用 |
| `profiles/desktop/` | 桌面版 profile(cordis.yml + patch) | 否 |

`projectKey(cwd)` 规则(源码 `session-persistence-jsonl/src/format.ts`):路径分隔符 `/ \ :` 连续折叠为一个 `-`,字符集 `[A-Za-z0-9._-]` 外的字符转 `~XXXX`(4 位大写十六进制码点),去掉前导 `-`,包 `--...--`,截断 251;无 cwd 的会话放 `_no-cwd`。示例:`D:\projects\web-apps` → `--D-projects-web-apps--`。

会话目录名 = session id 原样(`session-<uuid>`,见 `encodeSegment`);同一会话目录可同时存在多代文件(如 v3 迁移到 v4 后旧文件保留),**读取时取最高代**。

## 三、转录格式(v4)

### 物理层:多帧 zstd

- 压缩配置 `zstd | none`,默认 zstd,文件名后缀 `.zstd`。
- 写入按批次(≤200ms 合批)追加**独立 zstd 帧**,文件 = 帧拼接。源码有 `scanZstdFrames`/`decompressZstdPrefix` 配套(live/截断读取)。
- **实测**:187KB 文件含 144 帧、解出 261 行事件;Node `zlib.zstdDecompressSync` 只解第一帧(官方 API 限制,拼接帧只出 1 行)。
- Rust 读取要求:`ruzstd` 逐帧解码直到 EOF;截断尾帧应容忍(解到能解的位置),正好匹配"live 会话部分可读"。
- 写并发:写锁是 Windows 内核命名信号量(内核对象、不占文件),只读打开无锁冲突;write-behind 意味着最新 ≤200ms 的事件可能尚未落盘,可接受。

### 逻辑层:事件信封

```jsonc
{"type":"...","seq":0,"time":1791355573059,"data":{...}}   // time 为 Unix 毫秒
```

- 首行是 `session` header(无 seq):`{type:"session", version:4, id, createdAt, cwd, isSeeded, delegationDepth, agentPreset}`。
- seq 从 0 连续递增(实测 0..260 无洞)。
- surface 事件(`user/message`、`assistant/message`、`tool/result`、`system/message`、`developer/message`)额外带 `surfaceOp`("append" 或 `{op:"replace",startSeq,endSeq}`)与 `sourceEventSeqs`;**compaction 用 replace 折叠历史**,读取器若不应用 replace 语义,compaction 后的会话会重复展示被折叠内容(本机无 compaction 样本,见风险)。
- 未知 `type` 若无 `ignorable: true`,官方语义要求拒读(防静默丢语义);Reins 侧的取舍:降级为 unsupported 事件展示而不是 fail 整个来源(v1 决策)。

### 实测事件分布(261 事件的真实会话)

tool/call 59、tool/result 59、session-log-deepseek/delivery-accepted 29、step/start 28、assistant/message 28、step/end 28、agent/inbox/spliced 11、user/message 7、session/title 2、turn/start+turn/end 各 1,以及 header 后的 permission/preset、sandbox/mode、approval/policy、system/message、request/header、request/context、session/title-llm-request 各 1。

### 关键事件结构(实测样本,已去除内容噪音)

`user/message`:
```jsonc
{"type":"user/message","seq":8,"time":...,"data":{
  "content":[{"type":"text","text":"分析当前项目"}],
  "source":{"kind":"user","rpcId":"...","clientTimeZone":"Asia/Hong_Kong"},
  "role":"user","id":"<uuid>"},"surfaceOp":"append"}
```
- `source.kind` 实测分布:`user`(人类输入)、`agent-instructions`(AGENTS.md 注入)、`runtime-context`、`skill-catalog`。**人类输入与合成注入靠 source.kind 干净区分**。
- content 块:`text`(其他类型如 file 未在本机出现,走 unsupported 降级)。

`assistant/message`:
```jsonc
{"type":"assistant/message","seq":19,"time":...,"data":{
  "turn":1,"step":1,
  "message":{"role":"assistant","content":[
      {"type":"reasoning","text":""},
      {"type":"text","text":"..."},
      {"type":"tool-call","id":"call_00_...","name":"pwsh","arguments":"{...json字符串...}"}],
    "source":{"kind":"model","provider":"deepseek-account","model":"deepseek-flash",...},
    "id":"<uuid>"},
  "usage":{"inputTokens":10816,"outputTokens":113,"cacheReadTokens":0,"cacheWriteTokens":0,"totalTokens":10929},
  "stream":[/* 紧凑流记录,量大,v1 不展示 */]}}
```
- content 块三种:`reasoning`(→thinking)、`text`、`tool-call`(id/name/arguments-JSON字符串)。tool-call 在 assistant 消息里,而独立 `tool/call` 事件也记一次(内容同)——**渲染用 assistant/message 内的块即可,`tool/call` 事件用于配对校验**。
- `usage` 官方口径:字段 disjoint,inputTokens=未命中输入,reasoning 已含在 outputTokens。→ Reins `SessionTokenUsage` 直接 1:1(input_tokens=inputTokens, output_tokens=outputTokens, cache_read=cacheReadTokens, cache_write=cacheWriteTokens),无 zcode 那种拆算。

`tool/result`:
```jsonc
{"type":"tool/result","seq":21,"time":...,"data":{
  "turn":1,"step":1,
  "message":{"role":"tool","source":{"kind":"tool","callId":"call_00_..."},
    "toolCallId":"call_00_...","content":[{"type":"text","text":"Error: ..."}],
    "isError":true,"id":"<uuid>"}},
  "sourceEventSeqs":[20],"surfaceOp":"append"}
```
- 可选 `error:{name,code,reason}`(仅 isError 时)、`meta`(工具私有展示载荷,dsh-tool-fs 带 diff)。

`session/title`:`{data:{title:"分析当前项目",messageSeqs:[8],source:{kind:"fallback"}}}`,latest-wins。标题来源优先级:session/title 事件 > 首条人类 user/message > session id。

`turn/end`:`{data:{turn:1,reason:{kind:"completed"}}}`;`step/start|end`:`{data:{turn,step}}`。

`request/header`:`data.header.config` 含 `{provider, model, reasoningEffort, maxTokens}`(模型信息来源,可作事件/模型标记)。

### subagent 链接(源码权威定义,本机无样本)

- 子会话是**独立 session 文件**,header `delegationDepth > 0` 即子会话(根会话为 0)。
- 父日志 `subagent/catalog` 事件:`{childId: SessionId, childCreatedAt: number, version: 0|1, mode: 'one-shot'|'continuable'|'unknown', label?: string}`(源码 `packages/subagent/subagent/src/catalog.ts`)。
- 子日志开头有 `subagent/descriptor`(身份与生命周期,含可恢复组合)。
- family 聚合方案:扫描所有会话目录 → 以 header.delegationDepth 区分根/子 → 从根日志收集 `subagent/catalog.childId` 建立父子链 → 子会话按 family 归并展示(复用 `family_index.rs` 泛型,同 grokbuild 模式)。catalog 未覆盖的孤儿子会话:v1 隐藏(family root 缺失时子会话无处挂靠)。

### projcache 侧车(了解即可,v1 不依赖)

`storages/session_projcache/sessions/<id>.json`:`{version:7, record:{identity:{formatVersion,createdAt,cwd,isSeeded,inheritedEventCount}, rows:{title, tokenUsage:{totals:{uncachedInputTokens,outputTokens,cacheReadTokens,cacheWriteTokens}}, sessionListMetadata:{blank,lastPromptAt}, ...}}}`。
- 用途:dsh 自己的列表加速缓存,字段与转录一致(官方有跨版本读取兼容设计)。
- **实测发现:存在"已创建未落盘"会话**(projcache 有条目、`blank:true`、sessions 下无目录)——lazy materialization。Reins 以转录文件为准,天然跳过它们。
- Reins v1 选择转录直读,单一事实源,避免双源不一致;projcache 留作未来列表性能优化备选。

## 四、MCP 配置体系(Cordis patch,workspace 分发用)

dsh 无 mcpServers JSON/TOML。MCP server = Cordis 插件树里的一个 `@deepseek-ai/dsh-mcp-client` 条目,通过 patch 层插入。官方示例(`apps/cli/config/examples/mcp-memory/memorix.cordis.yml`):

```yaml
- insert:
    - id: memory-memorix
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: memorix
        transport: stdio
        command: memorix
        args: [serve]
        cwd: !!js process.cwd()
```

- patch 层位置:全局 `~/.dsh/cordis.patch.yml`(所有 profile 生效)或 `~/.dsh/profiles/<name>/cordis.patch.yml`(单 profile)。**Reins 写全局层**。
- patch 文件是操作列表(`- insert: [...]`,可能还有其他 op),**官方明确警告:merge 进已有文件,不能整文件覆盖**(用户可能已有无关 patch)。
- `!!js process.cwd()` 是 dsh 自有 YAML 扩展标签;Reins 写入时用普通字符串路径(cwd 必填字段,填 workspace 路径或用户 home)。
- stdio config 必填:`serverName`([A-Za-z0-9_-]{1,32},工具名前缀 mcp__<serverName>__*)、command、args、env、cwd、toolCallTimeoutMs、failOnStartupError。另有 `transport: streamable-http`(url/headers)。Reins 的 McpConfig(command/args/env)映射到 stdio 形态;toolCallTimeoutMs/failOnStartupError 用合理默认值(远端分发见本节 2026-10-09 补充)。
- 增删语义:按条目 `id` 定位(建议 Reins 用稳定 id 如 `reins-mcp-<serverName>`),remove op 的确切写法实现时查 Cordis patch 文法(`docs/cordis-primer.md` / vendor/cordis 源码)。
- **实现时踩坑(已解决)**:serde_yaml 0.9.34 解析 `!!js` 双叹号标签时直接丢弃标签(解析成普通 String),`!js` 单叹号才保留 Tagged。用户 patch 里的 `cwd: !!js process.cwd()` 若直接 serde_yaml 往返会被静默剥掉标签、改变 dsh 运行时语义。Reins 的解法:读入时把 `!!` 文本替换成哨兵单叹号标签(`!reins-yaml-bangbang-`)保住 Tagged 结构,写出时再反向还原(`mcps.rs` 的 `DSH_BANGBANG_SENTINEL`)。哨兵字符串出现在用户文件里的概率视为零。另:展示已有条目时 Tagged 值递归取内层(untag_yaml)。serde_yaml 往返同样不保留注释/锚点,与现有 JSON/TOML target 的行为一致,可接受。

### 补充(2026-10-09):streamable-http 契约已核验,Reins 支持远端分发

- 证据:本机安装产物 `~/.dsh/profiles/node_modules/@deepseek-ai/dsh-mcp-client/lib/types/index.d.ts`(dsh-mcp-client 0.1.5-rc.2)。`StreamableHttpConfig` 必填 `transport: 'streamable-http'`、`serverName`、`url`;`headers` 在 Input 形态为可选(Partial,resolved 有默认),`toolCallTimeoutMs`/`failOnStartupError` 亦可选;**远端形态没有 `command`/`args`/`env`/`cwd`**。类型注释原文:`Config for connecting to an MCP server over Streamable HTTP (SSE).` —— 该形态本身覆盖 SSE 系协议,因此 Reins 的 `McpTransport::Http` 与 `McpTransport::Sse` 都映射到 `streamable-http`。
- Reins 分发(此前仅 stdio,远端显式 bail;现已支持):

```yaml
- insert:
    - id: reins-mcp-<serverName>
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        transport: streamable-http
        serverName: probe
        url: https://example.com/mcp
        headers: { X-Key: value }   # 为空时省略该键
```

- 读取/反显:远端条目按 `config` 原样反读(条目 key 为清洗后的 serverName),inspect 归类与移除路径无需 dsh 特例;编辑页按钮不再对 dsh 的远端 MCP 置灰。

## 五、skills 目录

`packages/skill/skill-filesystem/src/index.ts` 实测根列表(按 rank):project `<root>/.dsh/skills`、`<root>/.agents/skills`;user `~/.dsh/skills`(DSH_HOME)、`~/.agents/skills`(DSH_AGENTS_HOME)。技能形态:目录带 `SKILL.md`(同 Reins 现有各 target 的约定)。
- Reins target 的 skillDir 用 `~/.dsh/skills`。
- 注意:dsh 同时扫 `~/.agents/skills`(与 codex target 的默认 skillDir 相同),UI 上两个 target 会显示重复安装,属预期行为,不需要去重逻辑。

## 六、settings.yaml 模型配置(providers 域用)

全局 `~/.dsh/settings.yaml`(支持 `$DSH_HOME` 重定向,默认 `~/.dsh`),密钥存 `.credentials.yaml`(**敏感边界:Reins providers 域本就只写路由不写 key**,保持一致)。生效无需重启。官方文档:`docs/user/guide/providers.md`、`docs/config-catalog.md#deepseek-aidsh-llm-pi-ai`。

```yaml
llm-pi-ai:
  providers:
    my-gateway:
      apiKeyEnv: EXAMPLE_API_KEY
      api: openai-completions        # 协议名同 pi
      baseURL: https://api.example.com/v1
      reasoning: high                # 路由级默认思考档(pi-ai ModelThinkingLevel,首档 off)
      models:
        - id: vendor/model-x
          contextWindow: 262144
          maxTokens: 32768
          input: [text, image]       # 请求模态;声明 image 才让手工声明的视觉模型可用
          reasoningEfforts:          # 档位名→线上拼写;未选档 null=支持但不发参数;false=显式非推理
            high: high
            max: max

agent-default-model:
  provider: my-gateway
  model: vendor/model-x
```

> 模型条目完整 schema 是 `PiAiModelProfile`(config-catalog.md,源码 packages/llm/llm-pi-ai/src/config.ts):id/name/contextWindow/maxTokens/input/reasoningEfforts/compat。接入首版只按示例块映射了 id/contextWindow/maxTokens,把图像/思考等级误标为"无落点",2026-10-07 晚已按官方接口补齐;「推理能力」布尔仍无独立字段(经等级字典或 false 间接表达),保留在 unwritten_model_fields 提示里。

- 路由名即 provider id,一旦使用不可改(请求与会话日志都引用它)。
- `agent-default-model` 是默认模型选择。
- 兼容性开关:`compat.supportsDeveloperRole`、`compat.maxTokensField`(网关拒绝请求时用)。
- **2026-10-07 晚实测(重要修正):桌面版对 settings.yaml 是「一次性导入」语义,不是常驻配置**。`packages/settings/settings/src/index.ts` 的 `importLegacyDocument`:启动时把 `<profile.home>/settings.yaml` 改名为 `settings.yaml.imported`(防重复导入),逐节 `update` 进当前 profile 的设置存储;被运行时组合拒绝的节只记 warn 日志、仅存在于改名文件里。实测:Reins 写出的路由+默认模型,`agent-default-model` 节成功落到 `~/.dsh/profiles/desktop/cordis.patch.yml`(provider: reins-glm/glm-5.3/reasoningEffort: high),但 `llm-pi-ai` 节被拒、未出现在 Settings→Models。拒绝的确切原因需 dsh 运行时日志(桌面版未找到持久化日志;凭据是请求时才校验——官方明确 "Model discovery returns the configured catalog regardless of credentials"——故 MISSING_CREDENTIAL 不是导入拒绝的原因,更可能是组合中设置条目 id 不匹配或配置校验拒绝)。接入时写的「settings.yaml 生效无需重启」对桌面版不成立;此前「运行时加载」unverified 项就此关闭,结论:Reins 写 settings.yaml 的路线对桌面版不可靠,用户侧闭环是在 dsh 的 Settings→Models 手工添加路由(以正确方式持久化进 profile patch,凭据由 dsh 自己保存)。

> **2026-10-07 深夜定稿(更新):providers 域已改走 Cordis patch 层写路由**(settings.yaml 路线废弃),且**路由与默认模型统一写 desktop profile patch 层**(profiles/desktop/cordis.patch.yml),不再写全局层——dsh Models 设置页的读写/回显都在 profile 层,写全局层会出现「路由生效但设置页看不到」的层错位(实测)。依据:dsh-base 组合里 `id: llm-pi-ai` 条目为休眠挂载,等 provider profiles 激活;patch 对已有行是 **id 定位整块 config 覆盖**;设置导入器自己也持久化到 profile patch(实证:agent-default-model 落在 profiles/desktop/cordis.patch.yml)。Reins 的写入分布:①路由(含 apiKeyEnv)→ profile patch 的 llm-pi-ai 行(apply 时顺带清理全局层残留的同 id 行,仅含 reins-* 路由或为空才清);②密钥 → dsh 托管的 .credentials.yaml,路由经 `apiKeyEnv: REINS_<平台>` 引用,与 dsh Models 页同构;③默认模型+默认思考档 → 同文件的 agent-default-model 行(dsh 默认档插件值是 high,不显式写 reasoningEffort 会回落它)。副作用:dsh CLI(非 desktop profile)读不到该路由,属已知取舍。

### 实测踩坑记录(2026-10-07 深夜,均已修复)

1. **凭据文档布局错 → 应用启动崩溃(最严重)**。`.credentials.yaml` 是 **version:1 分层文档**,顶层只允许 `version` / `refs` / `records` 三个键;把 `REINS_GLM: '...'` 追加到顶层 → 凭据服务启动校验失败 → `credentials` 服务挂 → 10 个依赖服务(connection/authorization/deepseekAccount/fileUploads 等)级联等待 → **整个应用无法启动**(crash log:`unknown top-level key "REINS_GLM"`)。凭据 ref 名文法宽松(`REF_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/`,即环境变量名),问题从来不是名字而是**位置**。refs 段内任意合法 env 名都可存(自定义路由存 key 的正确位置)。refs 清空后序列化为 `refs: {}`(不能是 null)。
2. **凭据解析序与校验时机的精确口径**:进程环境(胜) > .credentials.yaml(托管,可写) > 项目/.env > $DSH_HOME/.env;凭据缺失是**请求时**报 `MISSING_CREDENTIAL`,路由发现不受影响("Model discovery returns the configured catalog regardless of credentials")——但**凭据文档的键合法性是启动时强校验的**,两者别混。dsh 恢复按钮"back up profile patch, and restart"会把 profile patch 备份后清空(UI 偏好行会丢,需重设)。
3. **reasoningEfforts 形态错 → 路由挂载被拒**。pi-ai 规则:`reasoningEfforts` 字典**只声明要提供的档位,键缺席 = 不提供该档**;写了键但值为 null 表示"提供该档但不发参数",**只有 `off` 档允许 null**,其余档写了 null 会被拒(`reasoningEfforts.minimal needs the wire value dispatch should send`)。Pi 的 thinkingLevelMap 是"全量键+未选 null"形态,与 pi-ai(dsh)语义**不同,不可照搬**。档位枚举含 xhigh,键名即 off/minimal/low/medium/high/xhigh/max。
4. 排障入口:桌面版启动失败时,崩溃日志在桌面版 userData 的 `logs/` 目录(Windows 为 `%APPDATA%\@deepseek-ai\dsh-desktop\logs\crash-*-host.log`;对话框里的路径有截断,按此 glob 找)。

### 排障工作方式(约定)

排查 dsh 行为问题**优先读官方 GitHub 源码**:`git clone --depth 1 --single-branch https://github.com/deepseek-ai/deepseek-harness.git <临时目录>`,直接读 `packages/*/src`、`docs/`(config-catalog、architecture、cordis-primer)与各包 README——注释即设计文档,可信度高。本地安装目录打包产物的反向提取只作网络不可用时的最后手段,且编译产物无注释、易误读。权威文件对照:

- 基础组合条目表:`packages/bundle/base/cordis.patch.yml`(生成图见 `apps/cli/composition.md`)
- profile 组装与层序:`apps/cli/src/profile-boot.ts`
- settings 导入/持久化:`packages/settings/settings/src/index.ts`
- 凭据布局与信任分层:`packages/credentials/credentials-local/src/index.ts`
- 会话格式目录:`docs/persistence-catalog.md`;配置目录:`docs/config-catalog.md`

### 设计意图:组合树与 profile 分层(官方注释提炼)

组合树 = **patch 层按序叠加,行级 last-write-wins,后到者的 config 整块替换先到者**(不合并)。应用顺序与设计用意(apps/cli/src/profile-boot.ts 权威注释):

1. **bundle 层**(profile 的 `dsh.profile.bundles` 顺序):官方维护的共享核心。dsh-base 以"ONE insert over the empty profile root"铺全部基础条目;模式差异放在各自 mode bundle,"keep any single row down to one bundle layer plus the user's"(单行的完整定义只在一层,避免多层拼凑)。
2. **profile patch**(`<dsh>/profiles/<name>/cordis.patch.yml`):该 profile 的应用设置持久化层——"Live Config forms persist through the active profile patch"。Models 页/设置表单的保存都写这里(按条目 id 行覆盖)。profile 即"模式":desktop = dsh-base + web-app + 实验 agent-team/auto-review;web/headless/sdk/acp 共享 dsh-base 各自叠加。
3. **home 级用户层**(`<dsh>/cordis.patch.yml`):"machine-local preferences that apply to every profile, so it **outranks** the per-profile layer"——跨 profile 的机器级手动偏好,优先级高于 profile 层。Reins 不占用此层写 LLM 配置(它是用户手动领域),只写 MCP(官方示例即在此层)。
4. **`--patch` overlays**:启动参数级,意图最明确,最高优先。

其余设计用意(官方注释原文提炼):

- **llm-pi-ai 休眠挂载**:"Which adapters exist is composition; which providers run is the user's settings document"——组合决定适配器存在性,用户设置决定运行哪些 provider;零路由休眠挂载,设置节供给 profiles 后路由活跃注册、清空即回落。
- **settings.yaml 一次性导入**:改名先于写入("renamed before the first write, so a partial import never repeats"),被组合拒绝的节只留日志;它是 legacy 迁移通道,不是常驻配置。
- **凭据信任分层**:"inherited environment wins … it cannot be edited from inside, so it must be *visibly* read-only";托管文档可写,"a key the Models page writes takes effect immediately even when an older key sits in the user's .env"。文档"holds nothing but credentials",刻意不充当环境层。
- **会话存储**:append-only 多帧 zstd + 写锁用内核信号量(不占文件)→ 读者永远可以无锁读;projection cache 写后置(200 事件/5s)供列表加速。
- **dsh-llm-deepseek 与 dsh-llm-pi-ai 是 design-verification twins**(官方 package.json 描述):同一 LLM seam 的两种实现,后者承载任意网关路由。

对 Reins 的落地含义:LLM 配置写 per-profile 层(与 Models 页同层,回显=生效);home 层仅 MCP;清理残留时只清自己写入的行。

## 七、其他探索发现(本次不用,备查)

- **官方测试 fixtures**:仓库 `packages/session/session-persistence-jsonl/tests/fixtures/`(含 v0 真实形状样本)、`packages/experimental/webworker-runtime/tests/fixtures/vfs-example/home/sessions/`(v2/v3 会话样本树)、`packages/session/session-projection-cache/tests/fixtures/v4-session-doc.json`。Rust 测试可借用其内容做 fixture。
- 会话格式版本机制:writer 常量在 `packages/core/session/src/types.ts` 的 `SESSION_FORMAT_VERSION`;`docs/persistence-changes/` 记录每代 schema 与迁移;`scripts/migrate-sessions-to-v4.ts` 是官方迁移器。
- 桌面版会话也走同一存储(实测桌面创建的会话在 `~/.dsh/sessions` 下,与 CLI 共用)。
- `session-log-deepseek/delivery-accepted` 事件:官方云同步投递确认(每批一条,占比不小),纯噪音,v1 忽略(可归事件但不进消息)。
- `agent/inbox/spliced`:用户排队消息的变更审计,与 user/message 内容重复,v1 忽略。
- `request/header`:含完整工具定义(tools 数组,每条带 JSON schema),量很大,v1 只提取模型信息(provider/model/reasoningEffort),不进 payload。
- win32 写入路径用 `MoveFileExW(MOVEFILE_WRITE_THROUGH)` 做持久化发布、目录创建走 staging+rename(源码 `win32.ts`),对只读读取无影响。
- `DSH_HOME` 环境变量:settings/skills 已确认支持;sessions root 由 profile 组合层传入(backend config `root` 必填、无默认),**默认 composition 是否尊重 DSH_HOME 未确认**——Reins v1 固定读 `~/.dsh/sessions`(与 README"固定读各工具默认 home"的既有口径一致)。

## 八、接入方案(v1,三块并行)

### A. Sessions 来源 + usage(核心)

后端(模板:`grokbuild.rs` 文件型+family、`zcode.rs` usage 归一):

- `session/model.rs`:`SourceApp::Dsh`(serde `dsh`)。
- `session/dsh.rs`(新建):
  - `root()` = `~/.dsh`(存在性判定用其下 `sessions` 子目录);
  - `list_entries`:walkdir 扫 `sessions/**/session-*/session.v*.jsonl(.zstd)`,同目录多代取最高代,解压→header+扫描→summary;family 按 delegationDepth+subagent/catalog 聚合;
  - 事件映射(下表);
  - `parse_agent_messages` 走 family 内子会话消息;
  - `usage_hours`:assistant/message.usage 按事件 time 归小时桶;
  - 多帧 zstd 解压:ruzstd 逐帧,截断容忍;
  - 时间戳:事件 time 毫秒,直接用;summary 的 created/updated 取 header.createdAt 与最大事件 time。
- `catalog.rs`(detect 线程+available_sources)、`mod.rs`(reader/clear_all_caches/delete_session bail)、`usage_stats.rs`(source_days_from_files 分支)。
- `Cargo.toml`:+`ruzstd`。
- 测试:fixture 用真实转录样本(小会话 14KB 压缩样本可直接内嵌解压后的 JSONL;或官方 fixtures);覆盖多帧解码、事件映射、family、usage、截断容忍。

事件映射表:

| dsh | Reins |
| --- | --- |
| `user/message`(source.kind=user) | user 消息 text 块 |
| `user/message`(source.kind=agent-instructions/runtime-context/skill-catalog/…) | 事件(kind=source.kind,summary 取 text 摘要) |
| `assistant/message` | 消息:reasoning→thinking、text→text、tool-call→function_call(text=arguments,tool_name,tool_call_id);同消息 usage 累计 |
| `tool/result` | function_call_output(tool_call_id=message.toolCallId,text=content 拼接,is_error=isError,payload 原样) |
| `session/title` | 标题(latest-wins,列表/详情用) |
| `turn/end`、`permission/preset`、`sandbox/mode`、`approval/policy`、`model/selection`、`request/context`(模型变化时)、`plan/*`、`todo/*`、`compaction/*` | 事件时间线 |
| `step/start|end`、`agent/inbox/spliced`、`session-log-deepseek/*`、`session/title-llm-request`、`llm/retry*` | 忽略(噪音/重复) |
| 未知 type | `ignorable:true`→跳过;否则 unsupported 事件 |

前端:`pnpm codegen` 重生成 `SourceApp.ts`;`source-app.ts` label "DeepSeek Harness";`model.ts` canDeleteSession=false+文案;`usage/model.ts` 颜色。

### B. Workspace target(skills + MCP)

- `workspace/types.rs`:`McpConfigType::Dsh`(serde `dsh`)。
- `targets.rs` builtin_target_defaults + project_agent_defaults:dsh 条目(skillDir `~/.dsh/skills`,configPath `~/.dsh/cordis.patch.yml`)。
- 项目层补充(2026-10-07 晚):dsh 的 skills 有项目 rank(`<项目>/.dsh/skills`、`<项目>/.agents/skills`,优先于用户级),已以「仅 skill 分发」形态加入 project_agent_defaults 与 AVAILABLE_PROJECT_AGENTS(config_path 留空,MCP 仍只有全局 Cordis patch 层)。
- `mcps.rs`:`McpConfigType::Dsh` 的读写器——YAML patch 列表按条目 id 增删改,不整文件覆盖;stdio 必填字段补默认(toolCallTimeoutMs 等查官方默认);preview 遵循现有交互。
- 前端:workspace/model.ts preset+AVAILABLE_PROJECT_AGENTS+TargetCreateDialog 分段按钮+文案;codegen。

### C. Providers 域

- `providers/apps/dsh.rs`(新建,模板:`pi.rs`,同为 YAML):读改 `~/.dsh/settings.yaml` 的 `llm-pi-ai.providers`+`agent-default-model`;只写路由不碰 `.credentials.yaml`。
- 注册:providers 的 app 枚举/catalog;前端 `providers/model.ts` label、`ProviderAppId`(codegen)。

### 验收

- `cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm test`、`pnpm check`、`pnpm codegen` 全绿。
- 手动:Reins 列表出现 dsh 来源与 3 个会话;大会话详情消息/事件/用量正确;usage 统计页含 dsh 曲线;workspace 对 dsh target 的 skills 安装与 MCP 写入产出合法 patch YAML;dsh 重启或下次会话能加载(用户侧验证 MCP 生效)。

## 九、v1 落地验收记录(2026-10-07)

- 实现:sessions 来源 + usage(`session/dsh.rs` 38KB)、workspace target(Cordis patch 读写 + skills)、providers 适配器(`providers/apps/dsh.rs`)全部落地;`ruzstd 0.9` 逐帧解码(帧边界由解码器消费字节判定,不扫 magic)。
- 自动化:`cargo test` 324 项全绿(含 dsh 多帧解码/截断容忍/事件映射/replace 折叠/family 孤儿隐藏/usage 归桶/patch 合并幂等/`!!js` 标签字节级保留等);`pnpm test` 100 项、`pnpm check`、`pnpm codegen` 全绿。
- 真实数据端到端(临时 ignored 冒烟,已删):本机 4 会话全部正确列出;大会话「分析当前项目」88 消息/12 事件,块类型映射正确;token 聚合 input=70765/output=6959/cache_read=1149312;usage 日序列 1 天;空会话回退 session id 标题。
- 已知基线问题(非本次引入):`src-tauri/src/tests/session_delete.rs` 的 codex/grokbuild 官方 CLI 删除测试用 unix-only API(`std::os::unix::fs::PermissionsExt`)且部分测试未加 `#[cfg(unix)]` 门控(首个测试有门控,`fake_grok_cli` 与两个 grokbuild 测试漏了),Windows 上 `cargo test` 编译不过;需补门控(那些测试造的是 sh 脚本假 CLI,Windows 本就不可运行)。
- unverified:dsh 运行时实际加载 Reins 写出的 MCP patch 与 settings.yaml 的端到端效果(需 dsh 侧重启/发起会话验证)。
- 已补验证(2026-10-07 晚,真实 subagent 会话):root 会话派生 3 个子会话(目录名为裸 UUID、无 session- 前缀),7 个转录文件正确折叠为 4 个列表条目;family 标题「分析当前项目 (+3 subagents)」;overview 500 消息/62 事件/4 agents;`subagent/catalog` 的 label 正确作为 agents 清单与 marker 标题(如「复核 pouch 实现细节」);每子会话一个 marker;按 agent session id 取子会话消息(89 条)正常;孤儿子会话隐藏逻辑未在本次样本中出现(无孤儿)。

## 十、风险与待验证

- **subagent 无实测样本**:catalog 事件结构来自源码,实现后需要用户跑一次含 subagent 的 dsh 任务验证 family 聚合。
- **compaction/surfaceOp replace 无样本**:v1 先实现 replace 语义(按 sourceEventSeqs 折叠),用官方 schema 推导,标注待验证。
- **v3 旧代共存无本机样本**:多代取最高代的逻辑以源码 filename 规则为准。
- ruzstd 多帧流式解码的 API 细节(逐帧循环 vs StreamingDecoder 自动续帧)实现时以测试为准。
- `content` 块类型本机只见 `text/reasoning/tool-call`;`file` 等未见类型走 unsupported 降级。
