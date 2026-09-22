use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::session;
use crate::session::model::{SessionEvent, SessionMessage, SessionSummary, SourceApp};
use crate::state;
use crate::support;
use crate::test_support::TestEnvGuard;

mod grokbuild;
mod pi;
mod session_cache;
mod session_delete;
mod session_index;
mod session_listing;
mod session_readers;
mod workspace_skills;

fn write_jsonl(path: &Path, lines: &[Value]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    for line in lines {
        serde_json::to_writer(&mut file, line)?;
        writeln!(&mut file)?;
    }

    Ok(())
}

fn seed_opencode_family(root_id: &str, child_id: &str) -> Result<()> {
    let db_path = session::opencode::db_path()?;

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            title TEXT,
            version TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            type TEXT NOT NULL,
            seq INTEGER NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        ",
    )?;

    connection.execute(
        "INSERT INTO session_v2 (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            root_id,
            "project-1",
            root_id,
            "/tmp/root",
            "Root task",
            "1",
            1_744_366_400_000_i64,
            1_744_366_450_000_i64
        ],
    )?;
    connection.execute(
        "INSERT INTO session_v2 (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            child_id,
            "project-1",
            root_id,
            child_id,
            "/tmp/root",
            "Child task",
            "1",
            1_744_366_401_000_i64,
            1_744_366_460_000_i64
        ],
    )?;

    seed_opencode_message_row(
        &connection,
        root_id,
        "root-msg-1",
        1_744_366_400_000,
        1,
        "user",
        &json!({
            "time": { "created": 1_744_366_400_000_i64 },
            "text": "Root question",
            "files": [],
            "agents": [],
        }),
    )?;
    seed_opencode_message_row(
        &connection,
        child_id,
        "child-msg-1",
        1_744_366_401_000,
        1,
        "assistant",
        &json!({
            "time": { "created": 1_744_366_401_000_i64 },
            "agent": "build",
            "content": [{ "type": "text", "text": "Child answer" }],
        }),
    )?;

    Ok(())
}

fn seed_opencode_message_row(
    connection: &Connection,
    session_id: &str,
    message_id: &str,
    message_time: i64,
    seq: i64,
    kind: &str,
    data: &Value,
) -> Result<()> {
    connection.execute(
        "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            message_id,
            session_id,
            kind,
            seq,
            message_time,
            message_time,
            serde_json::to_string(data)?
        ],
    )?;

    Ok(())
}

// 直接写最小 Codex rollout 夹具（session_meta 即满足列表/索引读取）；
// 列表排序取文件 mtime，按调用顺序写文件即可获得预期顺序。
fn seed_codex_session(home: &Path, session_id: &str) -> Result<PathBuf> {
    let path = home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    write_jsonl(
        &path,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": { "id": session_id, "cwd": "/tmp/workspace" }
        })],
    )?;
    Ok(path)
}

// 读取类测试的聚合视图：用生产读取接口拼出 summary + family 路径 +
// 完整消息 + 事件。分页上限取足够大的常量，避免长会话被截断。
struct DetailView {
    summary: SessionSummary,
    source_paths: Vec<String>,
    messages: Vec<SessionMessage>,
    events: Vec<SessionEvent>,
}

fn read_detail(reader: &dyn session::SessionReader, path: &Path) -> Result<DetailView> {
    const ALL: usize = 10_000;

    let overview = reader.parse_overview(path)?;
    let messages = reader.parse_messages_page(path, 0, ALL)?.messages;
    let events = reader.parse_events_page(path, 0, ALL)?.events;

    Ok(DetailView {
        summary: overview.summary,
        source_paths: overview.source_paths,
        messages,
        events,
    })
}
