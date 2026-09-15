use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread;

use anyhow::{Context, Result, anyhow};

use crate::state::session_index::{SessionFileCatalog, SessionIndexState};

use super::{
    SessionPage, SessionRefreshResult, SessionSummary, SourceApp, SourceStatus, SummaryAccumulator,
};

pub(crate) fn detect_sources_inner(state: &SessionIndexState) -> Result<Vec<SourceStatus>> {
    let codex_root = super::codex::root()?;
    let claude_root = super::claude_code::root()?;
    let opencode_root = super::opencode::root()?;
    let pi_root = super::pi::sessions_root()?;
    let grok_root = super::grokbuild::root()?;

    let inspections = thread::scope(|scope| {
        let codex_state = state.clone();
        let codex_handle = scope.spawn(move || {
            inspect_source(
                &codex_state,
                SourceApp::Codex,
                codex_root,
                Some("Using ~/.codex/sessions as the primary transcript source.".to_string()),
            )
        });
        let claude_state = state.clone();
        let claude_handle = scope.spawn(move || {
            inspect_source(
                &claude_state,
                SourceApp::ClaudeCode,
                claude_root,
                Some("Reading project-level session JSONL files.".to_string()),
            )
        });
        let opencode_state = state.clone();
        let opencode_handle = scope.spawn(move || {
            inspect_source(
                &opencode_state,
                SourceApp::OpenCode,
                opencode_root,
                Some("Reading sessions from ~/.local/share/opencode/opencode.db.".to_string()),
            )
        });
        let pi_state = state.clone();
        let pi_handle = scope.spawn(move || {
            inspect_source(
                &pi_state,
                SourceApp::Pi,
                pi_root,
                Some("Reading ~/.pi/agent/sessions JSONL session files.".to_string()),
            )
        });

        let grok_state = state.clone();
        let grok_handle = scope.spawn(move || {
            inspect_source(&grok_state, SourceApp::GrokBuild, grok_root, None)
        });

        vec![
            codex_handle.join(),
            claude_handle.join(),
            opencode_handle.join(),
            pi_handle.join(),
            grok_handle.join(),
        ]
    });

    let mut sources = Vec::with_capacity(inspections.len());

    for inspection in inspections {
        let inspection = inspection.map_err(|_| anyhow!("Source inspection thread panicked"))??;

        if let Some(catalog) = inspection.catalog {
            state.store_catalog(inspection.app, catalog)?;
        }

        sources.push(SourceStatus {
            app: inspection.app,
            available: inspection.available,
            root_path: inspection.root_path,
            session_count: inspection.session_count,
            note: inspection.note,
        });
    }

    Ok(sources)
}

pub(crate) fn clear_session_caches_inner(state: &SessionIndexState) -> Result<()> {
    state.clear()?;
    super::clear_all_caches()
}

// 列表请求的来源选择：单一来源，或跨来源合并视图（全局分页）。
pub(crate) enum SourceSelection {
    All,
    One(SourceApp),
}

impl SourceSelection {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "all" => Ok(Self::All),
            other => Ok(Self::One(SourceApp::from_str(other)?)),
        }
    }

    fn sources(&self) -> Vec<SourceApp> {
        match self {
            Self::All => available_sources(),
            Self::One(app) => vec![*app],
        }
    }

    fn requested_source(&self) -> Option<SourceApp> {
        match self {
            Self::All => None,
            Self::One(app) => Some(*app),
        }
    }
}

// 与 detect_sources_inner 的可用性判断一致：只看来源根目录是否存在。
fn available_sources() -> Vec<SourceApp> {
    [
        (SourceApp::Codex, super::codex::root()),
        (SourceApp::ClaudeCode, super::claude_code::root()),
        (SourceApp::OpenCode, super::opencode::root()),
        (SourceApp::Pi, super::pi::sessions_root()),
        (SourceApp::GrokBuild, super::grokbuild::root()),
    ]
    .into_iter()
    .filter_map(|(app, root)| root.ok().filter(|root| root.exists()).map(|_| app))
    .collect()
}

pub(crate) fn list_sessions_inner(
    state: &SessionIndexState,
    selection: SourceSelection,
    offset: usize,
    limit: usize,
    query: &str,
    reverse: bool,
    refresh: bool,
) -> Result<SessionPage> {
    match build_session_page(state, &selection, offset, limit, query, reverse, refresh) {
        Ok(page) => Ok(page),
        Err(error) if !refresh => {
            build_session_page(state, &selection, offset, limit, query, reverse, true)
                .with_context(|| format!("Failed to load cached session page: {error}"))
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn refresh_sessions_inner(
    state: &SessionIndexState,
    requested: SourceSelection,
    offset: usize,
    limit: usize,
    query: &str,
    reverse: bool,
) -> Result<SessionRefreshResult> {
    clear_session_caches_inner(state)?;
    let sources = detect_sources_inner(state)?;

    // 合并视图始终合并所有来源；单一来源保持"请求的不可用时回退到首个可用"。
    let selection = match requested {
        SourceSelection::All => SourceSelection::All,
        SourceSelection::One(app) => SourceSelection::One(pick_available_source(app, &sources)),
    };
    let page = list_sessions_inner(state, selection, offset, limit, query, reverse, false)?;

    Ok(SessionRefreshResult {
        selected_source: page.source_app,
        sources,
        page,
    })
}

pub(crate) fn build_summary(
    source_app: SourceApp,
    path: &Path,
    summary: SummaryAccumulator,
) -> Result<SessionSummary> {
    let source_session_id = summary
        .session_id
        .or_else(|| derive_session_id(path))
        .ok_or_else(|| anyhow!("Unable to determine session id for {}", path.display()))?;

    let title = summary
        .title
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| source_session_id.clone());

    Ok(SessionSummary {
        source_app,
        source_session_id,
        title,
        cwd: summary.cwd,
        git_branch: summary.git_branch,
        transcript_path: path.display().to_string(),
        created_at: summary.created_at,
        updated_at: summary.updated_at,
    })
}

pub(crate) fn update_summary_timestamp(
    summary: &mut SummaryAccumulator,
    value: &serde_json::Value,
) {
    let Some(timestamp) = value
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(crate::support::time::parse_timestamp)
    else {
        return;
    };

    summary.created_at = match summary.created_at {
        Some(current) => Some(current.min(timestamp)),
        None => Some(timestamp),
    };

    summary.updated_at = match summary.updated_at {
        Some(current) => Some(current.max(timestamp)),
        None => Some(timestamp),
    };
}

pub(crate) fn derive_session_id(path: &Path) -> Option<String> {
    let file_stem = path.file_stem()?.to_str()?;

    if file_stem.len() >= 36 {
        return Some(file_stem[file_stem.len() - 36..].to_string());
    }

    Some(file_stem.to_string())
}

struct SourceInspection {
    app: SourceApp,
    available: bool,
    root_path: Option<String>,
    session_count: usize,
    note: Option<String>,
    catalog: Option<SessionFileCatalog>,
}

// 合并视图里的条目：列表摘要的加载和解析按来源进行，所以条目要带上所属来源。
struct MergedEntry {
    source_app: SourceApp,
    entry: super::SessionFileEntry,
}

fn build_session_page(
    state: &SessionIndexState,
    selection: &SourceSelection,
    offset: usize,
    limit: usize,
    query: &str,
    reverse: bool,
    refresh: bool,
) -> Result<SessionPage> {
    let mut merged = collect_merged_entries(state, selection, refresh)?;

    // 单一来源的目录本身有序，保持原顺序；合并视图需要跨来源统一排序。
    if matches!(selection, SourceSelection::All) {
        merged.sort_by(|a, b| {
            b.entry
                .sort_timestamp
                .cmp(&a.entry.sort_timestamp)
                .then_with(|| a.entry.path.cmp(&b.entry.path))
        });
    }

    let entries: Vec<&MergedEntry> = if reverse {
        merged.iter().rev().collect()
    } else {
        merged.iter().collect()
    };

    if let Some(query) = normalize_session_query(query) {
        return build_filtered_session_page(selection, &entries, offset, limit, &query);
    }

    let total_count = entries.len();
    let offset = offset.min(total_count);
    let end = offset.saturating_add(limit).min(total_count);
    let sessions = entries[offset..end]
        .iter()
        .map(|merged| load_session_summary(merged.source_app, &merged.entry))
        .collect::<Result<Vec<_>>>()?;
    let has_more = end < total_count;

    Ok(SessionPage {
        source_app: selection.requested_source(),
        sessions,
        offset,
        limit,
        next_offset: has_more.then_some(end),
        total_count,
        has_more,
    })
}

fn build_filtered_session_page(
    selection: &SourceSelection,
    entries: &[&MergedEntry],
    offset: usize,
    limit: usize,
    query: &str,
) -> Result<SessionPage> {
    let sessions = entries
        .iter()
        .map(|merged| load_session_summary(merged.source_app, &merged.entry))
        .collect::<Result<Vec<_>>>()?;
    let matching_sessions = sessions
        .into_iter()
        .filter(|summary| session_matches_query(summary, query))
        .collect::<Vec<_>>();
    let total_count = matching_sessions.len();
    let offset = offset.min(total_count);
    let end = offset.saturating_add(limit).min(total_count);
    let has_more = end < total_count;

    Ok(SessionPage {
        source_app: selection.requested_source(),
        sessions: matching_sessions[offset..end].to_vec(),
        offset,
        limit,
        next_offset: has_more.then_some(end),
        total_count,
        has_more,
    })
}

fn collect_merged_entries(
    state: &SessionIndexState,
    selection: &SourceSelection,
    refresh: bool,
) -> Result<Vec<MergedEntry>> {
    let mut merged = Vec::new();
    for source_app in selection.sources() {
        let catalog = session_catalog_entries(state, source_app, refresh)?;
        merged.extend(catalog.entries.iter().map(|entry| MergedEntry {
            source_app,
            entry: entry.clone(),
        }));
    }
    Ok(merged)
}

fn session_catalog_entries(
    state: &SessionIndexState,
    source_app: SourceApp,
    refresh: bool,
) -> Result<SessionFileCatalog> {
    if !refresh {
        if let Some(catalog) = state.catalog(source_app)? {
            return Ok(catalog);
        }
    }

    let catalog = build_session_catalog(source_app)?;
    state.store_catalog(source_app, catalog.clone())?;
    Ok(catalog)
}

fn build_session_catalog(source_app: SourceApp) -> Result<SessionFileCatalog> {
    Ok(SessionFileCatalog {
        entries: super::reader(source_app).list_entries()?.into(),
    })
}

fn load_session_summary(
    source_app: SourceApp,
    entry: &super::SessionFileEntry,
) -> Result<SessionSummary> {
    entry
        .summary
        .clone()
        .map(Ok)
        .unwrap_or_else(|| super::reader(source_app).parse_summary(&entry.path))
}

fn normalize_session_query(query: &str) -> Option<String> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_lowercase())
}

fn session_matches_query(summary: &SessionSummary, query: &str) -> bool {
    summary.title.to_lowercase().contains(query)
        || summary.source_session_id.to_lowercase().contains(query)
}

fn inspect_source(
    state: &SessionIndexState,
    app: SourceApp,
    root: PathBuf,
    note: Option<String>,
) -> Result<SourceInspection> {
    let available = root.exists();
    let root_path = available.then(|| root.display().to_string());

    if !available {
        return Ok(SourceInspection {
            app,
            available: false,
            root_path,
            session_count: 0,
            note: None,
            catalog: None,
        });
    }

    if let Some(catalog) = state.catalog(app)? {
        return Ok(SourceInspection {
            app,
            available: true,
            root_path,
            session_count: catalog.entries.len(),
            note,
            catalog: Some(catalog),
        });
    }

    let catalog = build_session_catalog(app)?;

    Ok(SourceInspection {
        app,
        available: true,
        root_path,
        session_count: catalog.entries.len(),
        note,
        catalog: Some(catalog),
    })
}

fn pick_available_source(requested_source: SourceApp, sources: &[SourceStatus]) -> SourceApp {
    sources
        .iter()
        .find(|source| source.app == requested_source && source.available)
        .map(|source| source.app)
        .or_else(|| {
            sources
                .iter()
                .find(|source| source.available)
                .map(|source| source.app)
        })
        .unwrap_or(SourceApp::Codex)
}
