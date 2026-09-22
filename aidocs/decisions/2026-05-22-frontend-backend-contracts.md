# 前后端 Contract 对照

- 日期：2026-05-22
- 状态：生效中

## Session

| 前端位置 | 后端位置 | command / DTO | 说明 |
| --- | --- | --- | --- |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `detect_sources` | 返回 `SourceStatus[]` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `clear_session_caches` | 清空 session index 和 reader cache |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `list_sessions` | 返回 `SessionPage` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `refresh_sessions` | 返回 `SessionRefreshResult` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `get_session_overview` | 返回 `SessionOverview` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `get_session_messages` | 返回 `SessionMessagePage` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `get_session_events` | 返回 `SessionEventPage` |
| `src/features/sessions/api.ts` | `src-tauri/src/session/commands.rs` | `delete_session` | 返回 `DeleteSessionResult` |

### Session 关键 DTO

| 前端类型 | 后端类型 | 后端文件 |
| --- | --- | --- |
| `SourceApp` | `SourceApp` | `src-tauri/src/session/model.rs` |
| `SourceStatus` | `SourceStatus` | `src-tauri/src/session/model.rs` |
| `SessionOverview` | `SessionOverview` | `src-tauri/src/session/model.rs` |
| `SessionMessagePage` | `SessionMessagePage` | `src-tauri/src/session/model.rs` |
| `SessionEventPage` | `SessionEventPage` | `src-tauri/src/session/model.rs` |
| `SessionPage` | `SessionPage` | `src-tauri/src/session/model.rs` |
| `SessionRefreshResult` | `SessionRefreshResult` | `src-tauri/src/session/model.rs` |
| `DeleteSessionResult` | `DeleteSessionResult` | `src-tauri/src/session/model.rs` |

## Workspace

| 前端位置 | 后端位置 | command / DTO | 说明 |
| --- | --- | --- | --- |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `get_workspace_state` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `load_workspace_state` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `set_workspace_dir` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `create_workspace` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `add_workspace` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `read_workspace` | 返回 `ManagedWorkspace` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `update_workspace` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `remove_workspace` | 返回 `WorkspaceState` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `select_workspace_directory` | 返回 `WorkspaceSelection \| null` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `select_local_skill_source_directory` | 返回 `WorkspaceSelection \| null` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `select_target_skill_directory` | 返回 `WorkspaceSelection \| null` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `select_target_mcp_config_file` | 返回 `WorkspaceSelection \| null` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `inspect_workspace_state` | 返回 `WorkspaceInspection` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `discover_git_skills` / `discover_local_skills` / `filter_discovered_skills` / `import_discovered_skills` | 返回 `SkillDiscoveryResult` |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `load_skill_detail` / `sync_skill_source` / `update_skill_source` | skill 详情与来源同步 |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `create_workspace_target` / `update_workspace_target` / `delete_workspace_target` | target CRUD |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `import_workspace_skill` | workspace skill 导入 |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `delete_skill_source` / `delete_skill_sources` | source 删除（详见 2026-06-04 ADR） |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `create_workspace_mcp` / `update_workspace_mcp` / `delete_workspace_mcp` | MCP CRUD |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `import_skill_to_target` / `remove_skill_from_target` | skill 分发 |
| `src/features/workspace/api.ts` | `src-tauri/src/workspace/commands.rs` | `apply_mcp_to_target` / `remove_mcp_from_target` / `preview_mcp_target` | MCP 分发与预览 |

### Workspace 关键 DTO

| 前端类型 | 后端类型 | 后端文件 |
| --- | --- | --- |
| `WorkspaceState` | `WorkspaceState` | `src-tauri/src/workspace/types.rs` |
| `ManagedWorkspace` | `ManagedWorkspace` | `src-tauri/src/workspace/types.rs` |
| `WorkspaceSelection` | `WorkspaceSelection` | `src-tauri/src/workspace/types.rs` |
| `AgentTargetId` | `AgentTargetId` | `src-tauri/src/workspace/types.rs` |
| `McpConfigType` | `McpConfigType` | `src-tauri/src/workspace/types.rs` |
| `McpTransport` | `McpTransport` | `src-tauri/src/workspace/types.rs` |
| `SkillDiscoveryResult` | `SkillDiscoveryResultView` | `src-tauri/src/workspace/types.rs` |
| `SkillSourceSyncResult` | `SkillSourceSyncResult` | `src-tauri/src/workspace/types.rs` |
| `SkillApplyResult` | `SkillApplyResult` | `src-tauri/src/workspace/types.rs` |
| `SkillRemoveResult` | `SkillRemoveResult` | `src-tauri/src/workspace/types.rs` |
| `WorkspaceTargetMutationResult` | `WorkspaceTargetMutationResult` | `src-tauri/src/workspace/types.rs` |
| `WorkspaceSkillMutationResult` | `WorkspaceSkillMutationResult` | `src-tauri/src/workspace/types.rs` |
| `WorkspaceMcpMutationResult` | `WorkspaceMcpMutationResult` | `src-tauri/src/workspace/types.rs` |
| `McpTargetMutationResult` | `McpTargetMutationResult` | `src-tauri/src/workspace/types.rs` |
| `McpTargetPreviewResult` | `McpTargetPreviewResult` | `src-tauri/src/workspace/types.rs` |

## 约束

1. 前端只允许通过 `src/features/sessions/api.ts` 和 `src/features/workspace/api.ts` 直接 `invoke` Tauri command。
2. 新增 command 时，必须同步更新本表和对应 feature 的 API wrapper。
3. DTO rename 如果只发生在单侧，必须在 wrapper 层显式说明映射关系，不能静默漂移。

## 变更记录

- 2026-06-04 增补：`delete_skill_source` / `delete_skill_sources` 与 `SkillSourceManageDialog` 一同引入，详见 `2026-06-04-skill-source-management.md`。
