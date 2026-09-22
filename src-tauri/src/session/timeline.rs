use std::path::PathBuf;

use anyhow::Result;

use super::{SessionEventPage, SessionMessage, SessionMessagePage, SessionOverview, SourceApp};

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

// 子代理弹窗的取数入口：一次返回该 agent 的完整消息，与时间线分页进度无关
pub(crate) fn get_session_agent_messages_inner(
    source_app: SourceApp,
    source_session_id: &str,
    agent_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<Vec<SessionMessage>> {
    let path = resolve_session_path(source_app, source_session_id, transcript_path)?;
    super::reader(source_app).parse_agent_messages(&path, agent_session_id)
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

fn resolve_session_path(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<PathBuf> {
    if let Some(path) = transcript_path {
        if source_app == SourceApp::GrokBuild {
            // 折叠后的子代理会话路径仍属于同一 family，按成员 id 放行。
            let overview = super::reader(source_app).parse_overview(std::path::Path::new(path))?;
            anyhow::ensure!(
                overview.summary.source_session_id == source_session_id
                    || overview
                        .agents
                        .iter()
                        .any(|agent| agent.session_id == source_session_id),
                "Grok Build session id mismatch"
            );
        }
        return Ok(PathBuf::from(path));
    }

    super::reader(source_app).resolve_path(source_session_id)
}
