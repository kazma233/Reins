use anyhow::Result;

use super::{SessionEvent, SessionMessage, SessionOverview, SourceApp};

pub(crate) fn get_session_overview_inner(
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<SessionOverview> {
    super::reader(source_app).parse_overview(source_session_id)
}

// 详情正文全量返回:分组器与子代理入口配对都需要完整消息流,不再分页。
pub(crate) fn get_session_messages_inner(
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<Vec<SessionMessage>> {
    super::reader(source_app).parse_messages(source_session_id)
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
) -> Result<Vec<SessionEvent>> {
    super::reader(source_app).parse_events(source_session_id)
}
