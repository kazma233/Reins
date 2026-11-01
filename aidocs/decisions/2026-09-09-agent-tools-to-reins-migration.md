# 2026-09-09 agent-tools → Reins 改名与数据迁移

## 背景

项目从 sandbox 单仓独立，并更名为 Reins（原名 agent-tools 过于通用，且生态内已有多个同名项目）。
产品身份层全部更名；数据目录全新启用，不做旧数据兼容。

## 改名范围（身份层）

- 目录名：`agent-tools/` → `reins/`
- `tauri.conf.json`：productName `Reins`、identifier `me.kazma.reins`、窗口标题
- `package.json`：name `reins`
- `Cargo.toml`：包名 `reins`、lib crate `reins_core`
- 源码内全部自引用：注释、用户提示文案、测试临时目录前缀

## 存储契约（全新启用，不兼容旧目录）

| 内容 | 旧（agent-tools） | 新（Reins） |
| --- | --- | --- |
| 应用数据目录（config.yaml、git cache） | `~/Library/Application Support/agent-tools/` | `~/Library/Application Support/reins/` |
| 会话摘要缓存 | `~/.agent-tools/session-summary-cache_v2.db` | `~/.reins/session-summary-cache_v2.db` |
| Git 刷新标记 key | `agent-tools-last-fetch` | `reins-last-fetch` |
| 导入器版本标记 | `agent-tools/0.1.0` | `reins/0.1.0` |
| Pi 导入事件 customType | `agent-tools-import(-unknown-role)` | `reins-import(-unknown-role)` |

这些字符串写入用户磁盘或会话数据，后续改动即等价于再次换存储位置。

## 迁移步骤（旧 agent-tools 环境 → Reins）

1. **配置文件**：把 `~/Library/Application Support/agent-tools/config.yaml`
   移动/拷贝到 `~/Library/Application Support/reins/config.yaml`。
   若 Reins 已启动过会自动生成模板 config.yaml，直接用旧配置覆盖，模板可删。
2. **git cache（可选）**：默认不动，sources 在首次使用时按记录重新 clone。
   想省流量可把旧目录 `git/<owner>/<repo>/` 拷到新数据目录的 `git/` 下。
3. **会话摘要缓存** `~/.agent-tools/`：放弃，Reins 会自动重建索引。
4. **导入过的会话**：customType 改名后，旧导入事件在转录中的标签仍为旧值；
   读取侧无引用，功能不受影响，仅标签外观。
5. **identifier 变化**（`me.kazma.agent-tools` → `me.kazma.reins`）：
   WebView 本地存储重置，应用未在其中持久化重要数据。

## 验证清单

- 启动 Reins，targets / skills / mcp 面板能列出原配置的 sources、targets、mcps
- 历史会话列表能发现本机 Codex / Claude Code / OpenCode / Pi 会话
- 任一 git 源执行一次同步或「强制拉取」，确认 clone 与软链正常