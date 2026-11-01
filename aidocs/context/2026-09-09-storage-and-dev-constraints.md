# 工程约束

- 提交 Rust 代码前先运行 `cargo fmt --manifest-path src-tauri/Cargo.toml`。
- 前端引用 `src/shared` 下的模块一律用 `@shared/...` 别名（vite alias + tsconfig paths 已配置），不写多级相对路径。

## 存储位置契约

以下名称写入用户磁盘或会话数据，属产品契约，改动即换存储位置/标记：

- 应用数据目录：`~/Library/Application Support/reins/`（`APP_DIR_NAME`，config.yaml、git cache 所在）
- 会话摘要缓存：`~/.reins/session-summary-cache_v2.db`
- Git 刷新标记 key：`reins-last-fetch`
- 导入器版本标记：`reins/0.1.0`（`IMPORTER_VERSION`）
- Pi 导入事件 customType：`reins-import` / `reins-import-unknown-role`

2026-09-09 项目由 agent-tools 改名 Reins，存储位置全新启用，旧 `agent-tools` 数据目录与 `.agent-tools` 缓存不做迁移、不做兼容。
