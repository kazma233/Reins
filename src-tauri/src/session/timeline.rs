use std::path::PathBuf;

use anyhow::Result;

use super::{SessionDetail, SessionEventPage, SessionMessagePage, SessionOverview, SourceApp};

pub(crate) fn get_session_overview_inner(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<SessionOverview> {
    let path = resolve_session_path(source_app, source_session_id, transcript_path)?;
    super::reader(source_app).parse_overview(&path)
}

pub(crate) fn get_session_messages_inner(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<SessionMessagePage> {
    let path = resolve_session_path(source_app, source_session_id, transcript_path)?;
    super::reader(source_app).parse_messages_page(&path, offset, limit)
}

pub(crate) fn get_session_events_inner(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<SessionEventPage> {
    let path = resolve_session_path(source_app, source_session_id, transcript_path)?;
    super::reader(source_app).parse_events_page(&path, offset, limit)
}

pub(crate) fn get_session_inner(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<SessionDetail> {
    let path = resolve_session_path(source_app, source_session_id, transcript_path)?;
    super::reader(source_app).parse_detail(&path)
}

fn resolve_session_path(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<PathBuf> {
    if let Some(path) = transcript_path {
        return Ok(PathBuf::from(path));
    }

    super::reader(source_app).resolve_path(source_session_id)
}
