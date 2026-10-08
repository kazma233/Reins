use anyhow::Result;

use super::{SessionEventPage, SessionMessage, SessionMessagePage, SessionOverview, SourceApp};

pub(crate) fn get_session_overview_inner(
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<SessionOverview> {
    super::reader(source_app).parse_overview(source_session_id)
}

pub(crate) fn get_session_messages_inner(
    source_app: SourceApp,
    source_session_id: &str,
    offset: usize,
    limit: usize,
) -> Result<SessionMessagePage> {
    super::reader(source_app).parse_messages_page(source_session_id, offset, limit)
}

// 子代理弹窗的取数入口：一次返回该 agent 的完整消息，与时间线分页进度无关
pub(crate) fn get_session_agent_messages_inner(
    source_app: SourceApp,
    source_session_id: &str,
    agent_session_id: &str,
) -> Result<Vec<SessionMessage>> {
    super::reader(source_app).parse_agent_messages(source_session_id, agent_session_id)
}

pub(crate) fn get_session_events_inner(
    source_app: SourceApp,
    source_session_id: &str,
    offset: usize,
    limit: usize,
) -> Result<SessionEventPage> {
    super::reader(source_app).parse_events_page(source_session_id, offset, limit)
}
