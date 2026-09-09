use std::str::FromStr;

use anyhow::Result;

use crate::state::session_index::SessionIndexState;

use super::catalog::{
    SourceSelection, clear_session_caches_inner, detect_sources_inner, list_sessions_inner,
    refresh_sessions_inner,
};
use super::delete::delete_session_inner;
use super::import::{import_session_inner, preview_import_inner};
use super::model::{
    DeleteSessionResult, ImportPreview, ImportResult, SessionEventPage, SessionMessagePage,
    SessionOverview, SessionPage, SessionRefreshResult, SourceApp, SourceStatus,
};
use super::timeline::{
    get_session_events_inner, get_session_messages_inner, get_session_overview_inner,
};

const DEFAULT_SESSION_PAGE_SIZE: usize = 20;
const MAX_SESSION_PAGE_SIZE: usize = 200;
const DEFAULT_DETAIL_PAGE_SIZE: usize = 40;
const MAX_DETAIL_PAGE_SIZE: usize = 100;

#[tauri::command]
pub(crate) async fn detect_sources(
    state: tauri::State<'_, SessionIndexState>,
) -> std::result::Result<Vec<SourceStatus>, String> {
    let session_index_state = state.inner().clone();
    run_blocking(move || detect_sources_inner(&session_index_state)).await
}

#[tauri::command]
pub(crate) async fn clear_session_caches(
    state: tauri::State<'_, SessionIndexState>,
) -> std::result::Result<(), String> {
    let session_index_state = state.inner().clone();
    run_blocking(move || clear_session_caches_inner(&session_index_state)).await
}

#[tauri::command]
pub(crate) async fn list_sessions(
    source_app: String,
    offset: Option<usize>,
    limit: Option<usize>,
    query: Option<String>,
    reverse: Option<bool>,
    refresh: Option<bool>,
    state: tauri::State<'_, SessionIndexState>,
) -> std::result::Result<SessionPage, String> {
    let selection = SourceSelection::parse(&source_app).map_err(|error| error.to_string())?;
    let session_index_state = state.inner().clone();
    let offset = offset.unwrap_or_default();
    let limit = limit
        .unwrap_or(DEFAULT_SESSION_PAGE_SIZE)
        .clamp(1, MAX_SESSION_PAGE_SIZE);
    let query = query.unwrap_or_default();
    let reverse = reverse.unwrap_or(false);
    let refresh = refresh.unwrap_or(offset == 0);

    run_blocking(move || {
        list_sessions_inner(
            &session_index_state,
            selection,
            offset,
            limit,
            &query,
            reverse,
            refresh,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn refresh_sessions(
    source_app: String,
    offset: Option<usize>,
    limit: Option<usize>,
    query: Option<String>,
    reverse: Option<bool>,
    state: tauri::State<'_, SessionIndexState>,
) -> std::result::Result<SessionRefreshResult, String> {
    let selection = SourceSelection::parse(&source_app).map_err(|error| error.to_string())?;
    let session_index_state = state.inner().clone();
    let offset = offset.unwrap_or_default();
    let limit = limit
        .unwrap_or(DEFAULT_SESSION_PAGE_SIZE)
        .clamp(1, MAX_SESSION_PAGE_SIZE);
    let query = query.unwrap_or_default();
    let reverse = reverse.unwrap_or(false);

    run_blocking(move || {
        refresh_sessions_inner(
            &session_index_state,
            selection,
            offset,
            limit,
            &query,
            reverse,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_session_overview(
    source_app: String,
    source_session_id: String,
    transcript_path: Option<String>,
) -> std::result::Result<SessionOverview, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    run_blocking(move || {
        get_session_overview_inner(source, &source_session_id, transcript_path.as_deref())
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_session_messages(
    source_app: String,
    source_session_id: String,
    transcript_path: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> std::result::Result<SessionMessagePage, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    let offset = offset.unwrap_or_default();
    let limit = limit
        .unwrap_or(DEFAULT_DETAIL_PAGE_SIZE)
        .clamp(1, MAX_DETAIL_PAGE_SIZE);

    run_blocking(move || {
        get_session_messages_inner(
            source,
            &source_session_id,
            transcript_path.as_deref(),
            offset,
            limit,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_session_events(
    source_app: String,
    source_session_id: String,
    transcript_path: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> std::result::Result<SessionEventPage, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    let offset = offset.unwrap_or_default();
    let limit = limit
        .unwrap_or(DEFAULT_DETAIL_PAGE_SIZE)
        .clamp(1, MAX_DETAIL_PAGE_SIZE);

    run_blocking(move || {
        get_session_events_inner(
            source,
            &source_session_id,
            transcript_path.as_deref(),
            offset,
            limit,
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn preview_import(
    source_app: String,
    source_session_id: String,
    target_app: String,
    transcript_path: Option<String>,
) -> std::result::Result<ImportPreview, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    let target = SourceApp::from_str(&target_app).map_err(|error| error.to_string())?;
    run_blocking(move || {
        preview_import_inner(
            source,
            &source_session_id,
            target,
            transcript_path.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_session(
    source_app: String,
    source_session_id: String,
    target_app: String,
    transcript_path: Option<String>,
) -> std::result::Result<ImportResult, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    let target = SourceApp::from_str(&target_app).map_err(|error| error.to_string())?;
    run_blocking(move || {
        import_session_inner(
            source,
            &source_session_id,
            target,
            transcript_path.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_session(
    source_app: String,
    source_session_id: String,
    transcript_path: Option<String>,
    state: tauri::State<'_, SessionIndexState>,
) -> std::result::Result<DeleteSessionResult, String> {
    let source = SourceApp::from_str(&source_app).map_err(|error| error.to_string())?;
    let session_index_state = state.inner().clone();

    run_blocking(move || {
        delete_session_inner(
            &session_index_state,
            source,
            &source_session_id,
            transcript_path.as_deref(),
        )
    })
    .await
}

async fn run_blocking<T, F>(operation: F) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation().map_err(|error| error.to_string()))
        .await
        .map_err(|error| error.to_string())?
}
