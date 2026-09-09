# 前后端契约类型用 ts-rs 机器化生成

- 日期：2026-08-18
- 状态：生效中

## 背景

Rust 与 TS 各自手写一份镜像类型（workspace 域 36 个 + session 域 15 个），靠人肉同步，已经存在漂移风险。选型结论：ts-rs v12 只生成类型，`api.ts`（invoke 封装）保持手写，不用 tauri-specta（不想引入整套 event/command 绑定体系）。

## 决策

1. **单一事实源在 Rust**：契约类型（跨 invoke 边界的参数/返回值类型）在 `workspace/types.rs` / `session/model.rs` 上加 `#[derive(TS)]` + `#[ts(export, export_to = "../../src/features/{domain}/generated/")]`。`pnpm codegen`（= `cargo test export_bindings`）生成到前端 `src/features/{domain}/generated/`，提交进仓库。
2. **`types.ts` 变薄壳**：只保留纯前端类型（`WorkspaceTab`、`SkillSourceKind`、`SkillDiscoveryResult` 别名）+ `export * from "./generated"`。前端其余文件 import 路径零变化。
3. **不导出内部类型**：`Raw*` / `Resolved*` / snapshot / cache 等只活在后端的类型不进契约集。

## 关键取舍（ts-rs v12 实测）

- **i64/u64 钉死为 `number`**：ts-rs 默认映射 bigint，但 Tauri wire 上是 JSON number。用 `#[ts(type = "number | null")]` 逐字段固化（7 处时间戳/超时字段），不依赖 `TS_RS_LARGE_INT` 环境变量。
- **payload 保持 `unknown | null`**：ts-rs 的递归 `JsonValue` 与 Vue 深层 `UnwrapRef`（`ref<SessionMessage[]>` 每处都触发）互相递归，产生 TS2589。opaque 化是与手写契约一致的正确形态。
- **optional 语义**：`#[ts(optional)]` 只能用于 Option 字段；`#[ts(optional = nullable)]` 生成 `?: T | null`。按手写契约逐字段选择形态。
- **transparent newtype**：v12 无 `#[ts(transparent)]`，用 `#[ts(type = "string")]`；`AgentTargetId` 的 serde 编解码手写为字符串，以保持 JSON 形态并避免 ts-rs 解析 `#[serde(transparent)]` 的 warning。
- **export_to 相对 `TS_RS_EXPORT_DIR`**（默认 `src-tauri/bindings/`）而非 .rs 文件自身路径。

## 已知限制

- ~~`ImportPreview.importLevel` 在 Rust 侧是 `String`，生成物因此是 `string`~~（2026-08-19 已解决：Rust 侧建成 `ImportLevel` enum 并 `#[ts(export)]`，生成物恢复 `"full" | "partial" | "unsupported"` 字面量 union，前端 `formatImportLevel` 的 default 兜底分支已删。）
- ts-rs 不生成 barrel，`generated/index.ts` 为手写聚合：**新增 `#[ts(export)]` 类型时需补一行 re-export**。

## 生效约束

- 改契约类型后必须跑 `pnpm codegen` 并把生成物一起提交；PR 里生成物 diff 与 Rust 类型 diff 应一一对应。
- 契约形态调整（optional / number / opaque）只改 Rust 侧 ts 属性，不手改生成文件。

## 验证

- `cargo check` 无 ts-rs serde warning、`cargo test --lib workspace` 60 通过、`cargo test export_bindings` 51 通过、codegen 幂等（重跑零变化）。
- `vue-tsc --noEmit` 零错误、vitest 9 通过、`pnpm build` 成功；api.ts / 组件 / stores 零改动，`AgentTargetId` 的 JSON 仍为字符串，其他 serde 属性未改。
