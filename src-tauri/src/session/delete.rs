use anyhow::Result;

use crate::state::session_index::SessionIndexState;

use super::{DeleteSessionResult, SourceApp};

pub(crate) fn delete_session_inner(
    state: &SessionIndexState,
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<DeleteSessionResult> {
    let path = if let Some(path) = transcript_path {
        std::path::PathBuf::from(path)
    } else {
        super::reader(source_app).resolve_path(source_session_id)?
    };
    let overview = super::reader(source_app).parse_overview(&path)?;

    super::delete_session(source_app, &path)?;
    state.clear()?;
    super::clear_all_caches()?;

    Ok(DeleteSessionResult {
        source_app,
        deleted_session_id: overview.summary.source_session_id,
        deleted_paths: overview.source_paths,
    })
}
