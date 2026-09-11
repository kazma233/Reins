pub mod logger;
mod session;
mod state;
mod support;
mod workspace;

pub fn run() {
    logger::log_info("application start");

    tauri::Builder::default()
        .manage(workspace::WorkspaceConfigStore::app())
        .manage(state::session_index::SessionIndexState::default())
        .manage(state::skill_discovery::SkillDiscoveryState::default())
        .invoke_handler(tauri::generate_handler![
            session::commands::detect_sources,
            session::commands::clear_session_caches,
            session::commands::list_sessions,
            session::commands::refresh_sessions,
            session::commands::get_session_overview,
            session::commands::get_session_messages,
            session::commands::get_session_agent_messages,
            session::commands::get_session_events,
            session::commands::preview_import,
            session::commands::import_session,
            session::commands::delete_session,
            workspace::commands::get_workspace_state,
            workspace::commands::select_project_path,
            workspace::commands::select_local_skill_source_directory,
            workspace::commands::select_target_skill_directory,
            workspace::commands::select_target_mcp_config_file,
            workspace::commands::discover_git_skills,
            workspace::commands::discover_local_skills,
            workspace::commands::filter_discovered_skills,
            workspace::commands::import_discovered_skills,
            workspace::commands::import_batch_git_skills,
            workspace::commands::create_workspace_target,
            workspace::commands::update_workspace_target,
            workspace::commands::delete_workspace_target,
            workspace::commands::create_workspace_mcp,
            workspace::commands::update_workspace_mcp,
            workspace::commands::delete_skill_source,
            workspace::commands::delete_skill_sources,
            workspace::commands::delete_workspace_mcp,
            workspace::commands::apply_mcp_to_target,
            workspace::commands::remove_mcp_from_target,
            workspace::commands::preview_mcp_target,
            workspace::commands::create_workspace_project,
            workspace::commands::update_workspace_project,
            workspace::commands::delete_workspace_project,
            workspace::commands::get_sync_target_options,
            workspace::commands::get_sync_skill_options,
            workspace::commands::sync_source_to_targets,
            workspace::commands::preview_source_sync_conflicts,
            workspace::commands::remove_source_sync,
            workspace::commands::refresh_git_skill_source,
            workspace::commands::update_skill_source,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run reins");
}

#[cfg(test)]
#[path = "tests/support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
