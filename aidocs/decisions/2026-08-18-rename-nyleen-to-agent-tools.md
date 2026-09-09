# 项目从 nyleen 改名为 agent-tools

- 日期：2026-08-18
- 状态：生效中

## 背景

项目原名 `nyleen`，体现在：
- Rust crate name / lib name
- Tauri `productName` / `identifier` / 窗口 title
- 用户数据目录 `~/Library/Application Support/nyleen/`（config.yaml + git cache）
- 导入会话时写入第三方工具的 `originator` / `entrypoint` 字段
- `IMPORTER_VERSION = "nyleen/0.1.0"`
- 文档 / 测试代码中的临时目录前缀

改名原因：`agent-tools` 更直白地反映产品定位（管理 agent 的 skills 与 mcp 配置分发），`nyleen` 是无语义代号。

## 决策

### 1. 全量替换，破坏性改动

按 AGENTS.md「不做兼容逻辑，每次出现破坏改动，提示即可」原则，**不做运行时迁移**，直接全量改名。

### 2. 改动范围

**后端 Rust**
- `Cargo.toml`：`name = "agent-tools"`，`lib.name = "agent_tools_core"`（Rust lib name 不能有连字符，且需与 bin 产物名区分）
- `main.rs`：`nyleen::` → `agent_tools_core::`（3 处）
- `lib.rs`：panic 文案
- `workspace/mod.rs`：`APP_DIR_NAME = "agent-tools"` + 路径注释
- `workspace/types.rs`：注释
- `logger.rs`：`APP_DIR_NAME = "agent-tools"`（日志目录）
- `session/model.rs`：`IMPORTER_VERSION = "agent-tools/0.1.0"`
- `session/import.rs`：warning 字符串
- `session/codex.rs`：`originator` 字段值
- `session/claude_code.rs`：`entrypoint` 字段值
- `session/opencode.rs`：临时文件前缀

**Tauri 配置**
- `tauri.conf.json`：`productName` / `identifier: me.kazma.agent-tools` / 窗口 title: "Agent Tools"

**前端 TS**
- `session-detail-helpers.ts`：warning 字符串（必须与后端 `import.rs` 一致，用于翻译映射）

**测试代码**
- 6 个测试文件中的临时目录前缀和测试数据

**文档**
- `README.md`：标题 + 6 处路径
- `AGENTS.md`：标题

### 3. 保留的 nyleen 字样

- `README.md` 第 70 行 `nyleen.yaml（历史命名）`——解释旧字段废弃，加「（历史命名）」说明
- `aidocs/decisions/` 下历史决策文档——按归档原则保留原貌

## 影响（破坏性）

### 用户数据路径变更

| 系统 | 旧路径 | 新路径 |
|------|--------|--------|
| macOS | `~/Library/Application Support/nyleen/` | `~/Library/Application Support/agent-tools/` |
| Linux | `~/.config/nyleen/` | `~/.config/agent-tools/` |
| Windows | `%APPDATA%\nyleen\` | `%APPDATA%\agent-tools\` |

旧用户升级后，应用会按新路径重新初始化。旧 `config.yaml` 和 `git/` 缓存不会自动迁移。

### Tauri identifier 变更

`me.kazma.nyleen` → `me.kazma.agent-tools`，macOS 视为不同 app（旧 app bundle 与新 app 数据隔离）。

### 软链断裂

target 里已分发的 skill 软链存的是 **canonicalize 后的绝对路径**（如 `/Users/<user>/Library/Application Support/nyleen/git/<owner>/<repo>/<skill>/`）。旧路径被 mv 走后软链会断，inspect 时 state 显示 `broken`。

迁移数据迁移命令见 [runbooks/migrate-from-nyleen.md](../runbooks/migrate-from-nyleen.md)。

## 验证

- `cargo check` 通过
- `cargo test` 38 passed / 0 failed
- `cargo fmt --check` 通过
- `pnpm vue-tsc --noEmit` 零错误
- `pnpm check`（完整）通过
