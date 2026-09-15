# MCP 配置字段映射缺口

本文件记录一个已知问题与后续优化方向：各 agent 的 MCP 配置字段并不一致，当前用 `configType` 写死映射，多处不一致会**静默失效**（写入成功、行为不符），后续要改为按 Reins 自己的 MCP 配置动态映射到各 agent 字段。

最后核实：2026-09-15，实测版本 Grok 1.0.30、Codex 0.149.1、OpenCode 1.18.30。Claude Code、ZCode 本机没有 CLI，未实测。

## 1. 问题

Reins 的 MCP 领域模型是统一口径（`mcps:` 下的 name、enabled、transport、command/args/env、url/headers、timeout）。写出时按 target 的 `mcp.config_type` 选模板（`src-tauri/src/workspace/mcps.rs`），读取时按 target 的 `config_prefix` 取原始值。

`configType` 让写入端可判定，但把「agent 的字段口径」固化成了全局枚举：target 名称允许自定义，无法按名字挑模板；每接一个新 agent 或某个 agent 改字段，就要再加类型，并同时改写入、预览、反读和前端说明。

## 2. Reins 当前写出口径（现状）

canonical 字段 → 实际写出：

| canonical | `common` TOML | `common` JSON | `opencode` JSON | `grokbuild` TOML |
| --- | --- | --- | --- | --- |
| 节点 | `mcp_servers.x` | `mcpServers.x` | `mcp.x` | `mcp_servers.x` |
| `enabled` | `enabled` | **不写** | `enabled` | `enabled` |
| `timeout`（ms） | `tool_timeout_sec` 浮点秒 | **不写** | `timeout` 整数毫秒 | `tool_timeout_sec` 整数秒（非整秒报错） |
| stdio | `command`、`args`、`env` | `type: stdio`、`command`、`args`、`env` | `type: local`、`command` 数组（含参数）、`environment` | `command`、`args`、`env` |
| 远程 | `url`、`http_headers` | `type: http/sse`、`url`、`headers` | `type: remote`、`url`、`headers` | `url`、`headers` |
| 文件格式 | JSON/TOML（按扩展名） | JSON/TOML | JSON（带 `$schema` 根） | 仅 TOML |

读取侧同样值得注意：

- `read_existing_mcp_entries` 原样返回目标文件里的值，**不做键名、单位或类型归一**。
- MCP inspection 只判断「同名条目是否存在」（`existing.contains_key(name)`），不校验字段。因此 `present` 只代表有条目，不代表字段真的生效。
- `common` TOML 只在 `args` / `env` 非空时写入这两项；`common` JSON 则总是写入（空数组/空对象）。

也就是说：`common` JSON 路径（Claude、ZCode）**根本不写 `enabled` 和 timeout**；`common` TOML 路径（Codex）写 `enabled`、浮点秒和 `http_headers`；`grokbuild` 写 `enabled`、整数秒和 `headers`。

## 3. 各 agent 实际口径与验证状态

| agent | 配置文件 / 节点 | stdio 字段 | 远程字段 | headers 键 | timeout | `enabled` | 验证状态 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Grok Build 1.0.30 | `$GROK_HOME/config.toml` / `mcp_servers` | `command`、`args`、`env`（另支持 `cwd`） | `url`、`headers`（另支持 `bearer_token_env_var`） | **`headers`**，`http_headers` 被静默忽略 | `tool_timeout_sec` 必须 `u64` 整数秒，浮点会让整个 server 被丢弃 | 支持 | 本机实测 |
| Codex 0.149.1 | `~/.codex/config.toml` / `mcp_servers` | `command`、`args`、`env`、`cwd` | `url`、`http_headers`、`env_http_headers`、`bearer_token_env_var` | **`http_headers`**，`headers` 被静默忽略 | `tool_timeout_sec` 接受浮点（1.5 可读回） | 支持（读回 `enabled: false`） | 本机实测 |
| OpenCode 1.18.30 | `~/.config/opencode/opencode.json` / `mcp` | `type: local`、`command` 数组、`environment` | `type: remote`、`url`、`headers` | `headers` | `timeout`（毫秒整数） | 支持 | 本机实测：解析结果原样保留，未知字段被静默剥离 |
| Claude Code | `~/.claude.json` / `mcpServers`（项目 `.mcp.json`） | Reins 写 `type: stdio` + `command/args/env` | Reins 写 `type: http/sse` + `url/headers` | Reins JSON 路径写 `headers` | **Reins 不写** | **Reins 不写** | 未实测（无 CLI）：需在隔离 HOME 用其 MCP 列表/校验命令核对 |
| ZCode | `~/.zcode/cli/config.json` / `mcp.servers` | 同 `common` JSON | 同 `common` JSON | 同 `common` JSON | **Reins 不写** | **Reins 不写** | 未实测（无 CLI） |
| Pi | 无 | — | — | — | — | — | Reins 只分发 skill，不写 MCP |

变量语义也不同（同一份配置在不同 agent 行为不一致）：

- Grok 在加载时展开 `${VAR}` / `${VAR:-default}`：`mcp list --json` 直接显示展开后的值；`{{session_id}}` 保持占位。
- Codex、OpenCode 的解析输出保留原文（`${TOKEN_VAR:-fallback}` 原样出现）；运行时的展开行为未验证。

## 4. 静默失败清单

这些情况都不会报错，属于「写入成功但行为不符」，是本次记录的核心原因：

1. **把 `http_headers` 写给 Grok** → server 能加载，headers 被丢弃（`grok mcp list --json` 里没有 `headers` 字段），远端认证静默失效。
2. **把 `headers` 写给 Codex** → server 能加载，`http_headers: null`，认证头同样静默失效。
3. **给 Grok 写浮点 `tool_timeout_sec`（如 `2.0`）** → 整个 server 被丢弃（`mcp list --json` 变成 `[]`），仅日志 WARN：`invalid type: floating point 2.0, expected u64 in tool_timeout_sec`。
4. **给 OpenCode 写不认识的字段**（例如把 `tool_timeout_sec` 当 opencode 字段）→ 未知字段被剥离，配置里看不到任何痕迹。
5. **`common` JSON 路径不写 `enabled`** → 在 Claude/ZCode 端无法表达「该 MCP 已禁用」，Reins 里禁用只影响 Reins 自己的判断。
6. **`common` JSON 路径不写 timeout** → 用户设置的超时对这两个 agent 无效且无提示。
7. **读取侧不做归一** → 即使字段写错，只要同名条目存在，inspection 仍显示 `present`。

## 5. 优化方向

让 Reins 的统一 MCP 配置成为唯一事实来源，各 agent 只提供一份字段映射适配器，写入与反读共用同一份映射：

- 字段名映射：`headers` ↔ `http_headers`、节点路径（`mcp_servers` / `mcpServers` / `mcp` / `mcp.servers`）等。
- 值转换：timeout 毫秒 → 目标单位与类型（整数秒 / 浮点秒 / 毫秒），不可表示时明确报错，不截断、不静默丢弃。
- 文件格式与根节点创建规则：TOML / JSON。
- 变量语义：是否由 agent 展开 `${VAR}` / `${VAR:-default}`，适配器要声明，预览时按目标实际行为展示。
- 能力表：每个 agent 支持哪些字段（stdio/HTTP/SSE、`cwd`、`env_http_headers`、`bearer_token_env_var`、`enabled`、timeout 等）。不支持或会被忽略的字段在预览里显式提示，而不是写出去被忽略。
- 反读归一：读取目标配置时按同一份映射解释回 canonical 字段，用于判断「已应用」和对比漂移；否则 `present` 判定会持续误导。
- 失败语义：目标 CLI 无法接受的值必须在预览阶段报错。

适配器的归属要显式声明（target 上选适配器，或按 agent 类型注册），不能靠 target 名称猜测；自定义 target 仍能指定使用哪个适配器。

## 6. 边界与验收

- 本次只登记问题与方向，不改变现有写入行为；`common`、`opencode`、`grokbuild` 的现有映射保持不变，避免在适配器契约确定前先动 Codex/Claude 路径。
- 改造前先确认契约：适配器数据形状、能力表字段、预览/反读如何共用、自定义 target 如何选择适配器、变量语义如何呈现。
- 验收标准：每个 agent 的 `apply → 反读 → remove` 幂等，且与目标 CLI 自身加载结果一致；所有静默丢失场景在预览中可解释。
- 已确认的字段差异必须写进适配器测试（含 `headers`/`http_headers` 互不识别、Grok 浮点超时被丢弃、OpenCode 未知字段被剥离、`common` JSON 缺 `enabled`/timeout），不能只依赖文档描述。

## 7. 复现命令（隔离 HOME，只用合成配置，不连网）

```bash
base=$(mktemp -d) && mkdir -p "$base/home" "$base/proj"

# Grok：headers vs http_headers、整数 vs 浮点 timeout
mkdir -p "$base/grok"
printf '[mcp_servers.remote]\nenabled=false\nurl="https://example.invalid/mcp"\nheaders={Authorization="Bearer t"}\ntool_timeout_sec=2\n' > "$base/grok/config.toml"
HOME="$base/home" GROK_HOME="$base/grok" grok --cwd "$base/proj" mcp list --json

# Codex：http_headers 生效、headers 被忽略、浮点 timeout 可读回
mkdir -p "$base/codex"
printf '[mcp_servers.remote]\nenabled=true\nurl="https://example.invalid/mcp"\nhttp_headers={Authorization="Bearer t"}\ntool_timeout_sec=1.5\n' > "$base/codex/config.toml"
HOME="$base/home" CODEX_HOME="$base/codex" codex --cd "$base/proj" mcp list --json

# OpenCode：字段是否被识别（未知字段会被剥离）
mkdir -p "$base/home/.config/opencode"
printf '{"mcp":{"probe":{"type":"local","command":["node"],"enabled":false}}}' > "$base/home/.config/opencode/opencode.json"
HOME="$base/home" XDG_CONFIG_HOME="$base/home/.config" sh -c "cd '$base/proj' && opencode debug config"
```

## 相关

- `aidocs/context/grokbuild-integration-analysis.md`：Grok Build 接入与 MCP 格式实现。
- `aidocs/context/2026-09-09-target-config-paths.md`：各 target 的全局/项目配置路径与节点。
