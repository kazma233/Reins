use std::collections::BTreeMap;

use anyhow::Result;
use tauri::async_runtime;

use crate::logger;
use crate::state::skill_discovery::SkillDiscoveryState;

use super::{
    BatchGitSkillImportInput, BatchGitSkillImportResult, McpConfigType, McpTargetMutationResult,
    McpTargetPreviewResult, McpTransport, ProjectMutationInput, RawMcpConfig, RawTargetInput,
    SkillDiscoveryResultView, SkillSourceMutationInput, SkillSyncItem, SourceSyncConflict,
    SourceSyncInput,
    SourceSyncResult, SyncSkillOptionsResult, SyncTargetOption, WorkspaceConfigStore,
    WorkspaceMcpMutationResult, WorkspaceState, WorkspaceTargetMutationResult,
    apply_mcp_to_target_inner, build_sync_skill_options, build_sync_target_options,
    create_workspace_mcp_inner, create_workspace_project_inner, create_workspace_target_inner,
    delete_skill_source_inner, delete_skill_sources_inner, delete_workspace_mcp_inner,
    delete_workspace_project_inner, delete_workspace_target_inner, discover_git_skills_inner,
    discover_local_skills_inner, filter_discovered_skills_inner, import_batch_git_skills_inner,
    import_discovered_skills_inner, preview_mcp_target_inner, preview_source_sync_conflicts_inner,
    refresh_git_skill_source_inner, remove_mcp_from_target_inner, remove_source_sync_inner,
    remove_target_skill_link_inner, select_local_skill_source_directory_inner,
    select_project_path_inner,
    select_target_mcp_config_file_inner, select_target_skill_directory_inner,
    sync_source_to_targets_inner, update_skill_source_inner, update_workspace_mcp_inner,
    update_workspace_project_inner, update_workspace_target_inner, workspace_state_inner,
};

#[tauri::command]
pub(crate) async fn get_builtin_target_preset(
    target_id: String,
) -> std::result::Result<super::TargetConfigView, String> {
    run_blocking(move || super::targets::builtin_target_preset_inner(&target_id)).await
}

#[tauri::command]
pub(crate) async fn get_workspace_state(
    store: tauri::State<'_, WorkspaceConfigStore>,
) -> std::result::Result<WorkspaceState, String> {
    let store = store.inner().clone();
    run_blocking(move || workspace_state_inner(&store)).await
}

#[tauri::command]
pub(crate) async fn select_project_path(
    project_path: Option<String>,
) -> std::result::Result<Option<super::WorkspaceSelection>, String> {
    run_blocking(move || select_project_path_inner(project_path.as_deref())).await
}

#[tauri::command]
pub(crate) async fn select_local_skill_source_directory(
    source_path: Option<String>,
) -> std::result::Result<Option<super::WorkspaceSelection>, String> {
    run_blocking(move || select_local_skill_source_directory_inner(source_path.as_deref())).await
}

#[tauri::command]
pub(crate) async fn select_target_skill_directory(
    current_path: Option<String>,
) -> std::result::Result<Option<super::WorkspaceSelection>, String> {
    run_blocking(move || select_target_skill_directory_inner(current_path.as_deref())).await
}

#[tauri::command]
pub(crate) async fn select_target_mcp_config_file(
    current_path: Option<String>,
) -> std::result::Result<Option<super::WorkspaceSelection>, String> {
    run_blocking(move || select_target_mcp_config_file_inner(current_path.as_deref())).await
}

#[tauri::command]
pub(crate) async fn apply_mcp_to_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    server_name: String,
    target_id: String,
) -> std::result::Result<McpTargetMutationResult, String> {
    let store = store.inner().clone();
    run_blocking(move || apply_mcp_to_target_inner(&store, &server_name, &target_id)).await
}

#[tauri::command]
pub(crate) async fn remove_mcp_from_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    server_name: String,
    target_id: String,
) -> std::result::Result<McpTargetMutationResult, String> {
    let store = store.inner().clone();
    run_blocking(move || remove_mcp_from_target_inner(&store, &server_name, &target_id)).await
}

#[tauri::command]
pub(crate) async fn preview_mcp_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    server_name: String,
    target_id: String,
) -> std::result::Result<McpTargetPreviewResult, String> {
    let store = store.inner().clone();
    run_blocking(move || preview_mcp_target_inner(&store, &server_name, &target_id)).await
}

#[tauri::command]
pub(crate) async fn discover_git_skills(
    store: tauri::State<'_, WorkspaceConfigStore>,
    state: tauri::State<'_, SkillDiscoveryState>,
    repo: String,
    reference: Option<String>,
) -> std::result::Result<SkillDiscoveryResultView, String> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    run_blocking(move || discover_git_skills_inner(&store, &state, &repo, reference.as_deref()))
        .await
}

#[tauri::command]
pub(crate) async fn discover_local_skills(
    state: tauri::State<'_, SkillDiscoveryState>,
    source_path: String,
) -> std::result::Result<SkillDiscoveryResultView, String> {
    let state = state.inner().clone();
    run_blocking(move || discover_local_skills_inner(&state, &source_path)).await
}

#[tauri::command]
pub(crate) async fn filter_discovered_skills(
    state: tauri::State<'_, SkillDiscoveryState>,
    discovery_id: String,
    include_name_patterns: Option<Vec<String>>,
    include_path_patterns: Option<Vec<String>>,
) -> std::result::Result<SkillDiscoveryResultView, String> {
    let state = state.inner().clone();
    run_blocking(move || {
        filter_discovered_skills_inner(
            &state,
            &discovery_id,
            include_name_patterns.as_deref(),
            include_path_patterns.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_discovered_skills(
    store: tauri::State<'_, WorkspaceConfigStore>,
    state: tauri::State<'_, SkillDiscoveryState>,
    discovery_id: String,
    include_name_patterns: Option<Vec<String>>,
    include_path_patterns: Option<Vec<String>>,
) -> std::result::Result<SkillDiscoveryResultView, String> {
    let store = store.inner().clone();
    let state = state.inner().clone();
    run_blocking(move || {
        import_discovered_skills_inner(
            &store,
            &state,
            &discovery_id,
            include_name_patterns.as_deref(),
            include_path_patterns.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_batch_git_skills(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: BatchGitSkillImportInput,
) -> std::result::Result<BatchGitSkillImportResult, String> {
    let store = store.inner().clone();
    run_blocking(move || import_batch_git_skills_inner(&store, input)).await
}

#[tauri::command]
pub(crate) async fn create_workspace_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    target_id: String,
    enabled: bool,
    skill_dir: String,
    config_path: Option<String>,
    mcp_config_prefix: String,
    mcp_config_type: McpConfigType,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!("create_workspace_target target_id={target_id}"));
    let store = store.inner().clone();
    run_blocking(move || {
        create_workspace_target_inner(
            &store,
            RawTargetInput {
                target_id,
                enabled,
                skill_dir,
                config_path,
                mcp_config_prefix,
                mcp_config_type,
            },
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_workspace_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    current_target_id: String,
    target_id: String,
    enabled: bool,
    skill_dir: String,
    config_path: Option<String>,
    mcp_config_prefix: String,
    mcp_config_type: McpConfigType,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!(
        "update_workspace_target current_target_id={current_target_id} target_id={target_id}"
    ));
    let store = store.inner().clone();
    run_blocking(move || {
        update_workspace_target_inner(
            &store,
            &current_target_id,
            RawTargetInput {
                target_id,
                enabled,
                skill_dir,
                config_path,
                mcp_config_prefix,
                mcp_config_type,
            },
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_workspace_target(
    store: tauri::State<'_, WorkspaceConfigStore>,
    target_id: String,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!("delete_workspace_target target_id={target_id}"));
    let store = store.inner().clone();
    run_blocking(move || delete_workspace_target_inner(&store, &target_id)).await
}

#[tauri::command]
pub(crate) async fn create_workspace_mcp(
    store: tauri::State<'_, WorkspaceConfigStore>,
    name: String,
    enabled: bool,
    transport: McpTransport,
    homepage: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
    timeout: Option<u64>,
) -> std::result::Result<WorkspaceMcpMutationResult, String> {
    logger::log_info(format!(
        "create_workspace_mcp name={name} transport={transport:?}"
    ));
    let store = store.inner().clone();
    run_blocking(move || {
        create_workspace_mcp_inner(
            &store,
            RawMcpConfig {
                name,
                enabled,
                transport,
                created_at: Some(super::current_timestamp_ms()),
                homepage,
                command,
                args,
                env,
                url,
                headers,
                timeout,
            },
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_workspace_mcp(
    store: tauri::State<'_, WorkspaceConfigStore>,
    server_name: String,
    name: String,
    enabled: bool,
    transport: McpTransport,
    homepage: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
    timeout: Option<u64>,
) -> std::result::Result<WorkspaceMcpMutationResult, String> {
    logger::log_info(format!(
        "update_workspace_mcp server_name={server_name} name={name} transport={transport:?}"
    ));
    let store = store.inner().clone();
    run_blocking(move || {
        update_workspace_mcp_inner(
            &store,
            &server_name,
            RawMcpConfig {
                name,
                enabled,
                transport,
                created_at: None,
                homepage,
                command,
                args,
                env,
                url,
                headers,
                timeout,
            },
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_skill_source(
    store: tauri::State<'_, WorkspaceConfigStore>,
    source_id: String,
) -> std::result::Result<WorkspaceState, String> {
    logger::log_info(format!("delete_skill_source source_id={source_id}"));
    let store = store.inner().clone();
    run_blocking(move || delete_skill_source_inner(&store, &source_id)).await
}

#[tauri::command]
pub(crate) async fn delete_skill_sources(
    store: tauri::State<'_, WorkspaceConfigStore>,
    source_ids: Vec<String>,
) -> std::result::Result<(), String> {
    logger::log_info(format!("delete_skill_sources count={}", source_ids.len()));
    let store = store.inner().clone();
    run_blocking(move || delete_skill_sources_inner(&store, source_ids)).await
}

#[tauri::command]
pub(crate) async fn delete_workspace_mcp(
    store: tauri::State<'_, WorkspaceConfigStore>,
    server_name: String,
) -> std::result::Result<WorkspaceMcpMutationResult, String> {
    logger::log_info(format!("delete_workspace_mcp server_name={server_name}"));
    let store = store.inner().clone();
    run_blocking(move || delete_workspace_mcp_inner(&store, &server_name)).await
}

#[tauri::command]
pub(crate) async fn create_workspace_project(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: ProjectMutationInput,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!(
        "create_workspace_project project_id={}",
        input.project_id
    ));
    let store = store.inner().clone();
    run_blocking(move || create_workspace_project_inner(&store, input)).await
}

#[tauri::command]
pub(crate) async fn update_workspace_project(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: ProjectMutationInput,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!(
        "update_workspace_project project_id={}",
        input.project_id
    ));
    let store = store.inner().clone();
    run_blocking(move || update_workspace_project_inner(&store, input)).await
}

#[tauri::command]
pub(crate) async fn delete_workspace_project(
    store: tauri::State<'_, WorkspaceConfigStore>,
    project_id: String,
) -> std::result::Result<WorkspaceTargetMutationResult, String> {
    logger::log_info(format!("delete_workspace_project project_id={project_id}"));
    let store = store.inner().clone();
    run_blocking(move || delete_workspace_project_inner(&store, &project_id)).await
}

#[tauri::command]
pub(crate) async fn get_sync_target_options(
    store: tauri::State<'_, WorkspaceConfigStore>,
) -> std::result::Result<Vec<SyncTargetOption>, String> {
    let store = store.inner().clone();
    run_blocking(move || build_sync_target_options(&store)).await
}

#[tauri::command]
pub(crate) async fn get_sync_skill_options(
    store: tauri::State<'_, WorkspaceConfigStore>,
    source_id: String,
) -> std::result::Result<SyncSkillOptionsResult, String> {
    let store = store.inner().clone();
    run_blocking(move || build_sync_skill_options(&store, &source_id)).await
}

#[tauri::command]
pub(crate) async fn sync_source_to_targets(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: SourceSyncInput,
) -> std::result::Result<SourceSyncResult, String> {
    let store = store.inner().clone();
    run_blocking(move || sync_source_to_targets_inner(&store, input)).await
}

#[tauri::command]
pub(crate) async fn preview_source_sync_conflicts(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: SourceSyncInput,
) -> std::result::Result<Vec<SourceSyncConflict>, String> {
    let store = store.inner().clone();
    run_blocking(move || preview_source_sync_conflicts_inner(&store, input)).await
}

#[tauri::command]
pub(crate) async fn remove_source_sync(
    store: tauri::State<'_, WorkspaceConfigStore>,
    source_id: String,
    target_ids: Vec<String>,
) -> std::result::Result<SourceSyncResult, String> {
    let store = store.inner().clone();
    run_blocking(move || remove_source_sync_inner(&store, &source_id, &target_ids)).await
}

#[tauri::command]
pub(crate) async fn remove_target_skill_link(
    store: tauri::State<'_, WorkspaceConfigStore>,
    target_id: String,
    destination_path: String,
) -> std::result::Result<SkillSyncItem, String> {
    let store = store.inner().clone();
    run_blocking(move || remove_target_skill_link_inner(&store, &target_id, &destination_path)).await
}

#[tauri::command]
pub(crate) async fn refresh_git_skill_source(
    store: tauri::State<'_, WorkspaceConfigStore>,
    source_id: String,
) -> std::result::Result<(), String> {
    let store = store.inner().clone();
    run_blocking(move || refresh_git_skill_source_inner(&store, &source_id)).await
}

#[tauri::command]
pub(crate) async fn update_skill_source(
    store: tauri::State<'_, WorkspaceConfigStore>,
    input: SkillSourceMutationInput,
) -> std::result::Result<WorkspaceState, String> {
    let store = store.inner().clone();
    run_blocking(move || update_skill_source_inner(&store, input)).await
}

async fn run_blocking<T, F>(operation: F) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    async_runtime::spawn_blocking(move || operation().map_err(|error| error.to_string()))
        .await
        .map_err(|error| error.to_string())?
}
