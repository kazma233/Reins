use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::session;
use crate::session::model::{
    ContentBlock, SessionDetail, SessionMessage, SessionSummary, SourceApp,
};
use crate::state;
use crate::support;
use crate::test_support::TestEnvGuard;

mod grokbuild;
mod pi;
mod session_cache;
mod session_delete;
mod session_import;
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

fn seed_opencode_session(detail: &SessionDetail, session_id: &str) -> Result<()> {
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

    let created_at = detail.summary.created_at.unwrap_or(0);
    let updated_at = detail.summary.updated_at.unwrap_or(created_at);
    let cwd = detail
        .summary
        .cwd
        .clone()
        .unwrap_or_else(|| "/tmp".to_string());

    connection.execute(
        "INSERT INTO session_v2 (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session_id,
            "project-1",
            session_id,
            cwd,
            detail.summary.title,
            "1",
            created_at,
            updated_at
        ],
    )?;

    for (index, message) in detail.messages.iter().enumerate() {
        let message_time = message.timestamp.unwrap_or(created_at + index as i64);
        let kind = if message.role == "user" {
            "user"
        } else {
            "assistant"
        };
        let data = if kind == "user" {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| block.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            json!({
                "time": { "created": message_time },
                "text": text,
                "files": [],
                "agents": [],
            })
        } else {
            json!({
                "time": { "created": message_time },
                "agent": "build",
                "content": seed_opencode_content(message),
            })
        };

        seed_opencode_message_row(
            &connection,
            session_id,
            &format!("msg-seed-{index}"),
            message_time,
            index as i64,
            kind,
            &data,
        )?;
    }

    Ok(())
}

// 归一化块 → v2 content 块数组，形状与真实库一致：文本/思考是
// {type,text}，工具调用与结果各自成 {type:"tool"} 块
fn seed_opencode_content(message: &SessionMessage) -> Vec<Value> {
    let mut content = Vec::new();

    for block in &message.blocks {
        match block.kind.as_str() {
            "text" | "input_text" | "output_text" => {
                if let Some(text) = block.text.clone() {
                    content.push(json!({ "type": "text", "text": text }));
                }
            }
            "thinking" => {
                if let Some(text) = block.text.clone() {
                    content.push(json!({ "type": "reasoning", "text": text }));
                }
            }
            "tool_use" | "function_call" => {
                content.push(json!({
                    "type": "tool",
                    "id": block.tool_call_id,
                    "name": block.tool_name,
                    "state": {
                        "status": "running",
                        "input": block.payload.as_ref().and_then(|payload| payload.get("input")).cloned().unwrap_or_else(|| json!({})),
                        "content": [],
                        "metadata": {},
                    },
                }));
            }
            "tool_result" | "function_call_output" => {
                content.push(json!({
                    "type": "tool",
                    "id": block.tool_call_id,
                    "name": block.tool_name,
                    "state": {
                        "status": "completed",
                        "input": {},
                        "content": [{ "type": "text", "text": block.text.clone().unwrap_or_default() }],
                        "metadata": {},
                    },
                }));
            }
            _ => {
                if let Some(text) = block.text.clone() {
                    content.push(json!({ "type": "text", "text": text }));
                }
            }
        }
    }

    content
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

fn has_codex_function_call_pair(session_id: &str, call_id: &str) -> Result<bool> {
    let path = session::codex::find_session_file(session_id)?;
    let mut has_call = false;
    let mut has_output = false;

    for line in BufReader::new(File::open(path)?).lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if value["type"] != "response_item" {
            continue;
        }

        let payload = &value["payload"];
        if payload["call_id"].as_str() != Some(call_id) {
            continue;
        }

        match payload["type"].as_str() {
            Some("function_call") => has_call = true,
            Some("function_call_output") => has_output = true,
            _ => {}
        }
    }

    Ok(has_call && has_output)
}

fn codex_has_event_type(session_id: &str, event_type: &str) -> Result<bool> {
    let path = session::codex::find_session_file(session_id)?;

    for line in BufReader::new(File::open(path)?).lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if value["type"] != "event_msg" {
            continue;
        }

        if value["payload"]["type"].as_str() == Some(event_type) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn codex_timestamps_are_monotonic(session_id: &str) -> Result<bool> {
    let path = session::codex::find_session_file(session_id)?;
    let mut previous = None;

    for line in BufReader::new(File::open(path)?).lines() {
        let value: Value = serde_json::from_str(&line?)?;
        let Some(raw_timestamp) = value["timestamp"].as_str() else {
            continue;
        };
        let Some(timestamp) = support::time::parse_timestamp(raw_timestamp) else {
            continue;
        };

        if previous.is_some_and(|current| timestamp < current) {
            return Ok(false);
        }

        previous = Some(timestamp);
    }

    Ok(true)
}

fn codex_thread_exists(session_id: &str) -> Result<bool> {
    let state_db = latest_codex_state_db()?;
    let connection = Connection::open(state_db)?;
    let count = connection.query_row(
        "SELECT COUNT(*) FROM threads WHERE id = ?1",
        params![session_id],
        |row| row.get::<_, i64>(0),
    )?;

    Ok(count > 0)
}

fn latest_codex_state_db() -> Result<PathBuf> {
    let root = session::codex::root()?;
    let mut candidates = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("state_") && name.ends_with(".sqlite"))
        })
        .collect::<Vec<_>>();

    candidates.sort();
    candidates
        .pop()
        .ok_or_else(|| anyhow!("Could not find Codex state_*.sqlite"))
}

fn sample_detail(source_app: SourceApp) -> SessionDetail {
    SessionDetail {
        summary: SessionSummary {
            source_app,
            source_session_id: "source-session".to_string(),
            title: "Hello from importer".to_string(),
            cwd: Some("/tmp/reins/workspace".to_string()),
            git_branch: Some("main".to_string()),
            transcript_path: "/tmp/source.jsonl".to_string(),
            created_at: Some(1_744_366_400_000),
            updated_at: Some(1_744_366_460_000),
        },
        source_paths: vec!["/tmp/source.jsonl".to_string()],
        messages: vec![
            SessionMessage {
                id: "message-1".to_string(),
                role: "user".to_string(),
                timestamp: Some(1_744_366_400_000),
                blocks: vec![ContentBlock {
                    kind: "text".to_string(),
                    text: Some("Hello from importer".to_string()),
                    tool_name: None,
                    tool_call_id: None,
                    is_error: None,
                    payload: None,
                }],
                session_id: None,
            },
            SessionMessage {
                id: "message-2".to_string(),
                role: "assistant".to_string(),
                timestamp: Some(1_744_366_410_000),
                blocks: vec![
                    ContentBlock {
                        kind: "thinking".to_string(),
                        text: Some("Need to inspect the workspace.".to_string()),
                        tool_name: None,
                        tool_call_id: None,
                        is_error: None,
                        payload: None,
                    },
                    ContentBlock {
                        kind: "tool_use".to_string(),
                        text: None,
                        tool_name: Some("Bash".to_string()),
                        tool_call_id: Some("call_import_1".to_string()),
                        is_error: None,
                        payload: Some(json!({
                          "input": {
                            "command": "pwd"
                          }
                        })),
                    },
                ],
                session_id: None,
            },
            SessionMessage {
                id: "message-3".to_string(),
                role: "tool".to_string(),
                timestamp: Some(1_744_366_420_000),
                blocks: vec![ContentBlock {
                    kind: "function_call_output".to_string(),
                    text: Some("/tmp/reins/workspace".to_string()),
                    tool_name: Some("Bash".to_string()),
                    tool_call_id: Some("call_import_1".to_string()),
                    is_error: None,
                    payload: Some(json!({
                      "output": "/tmp/reins/workspace"
                    })),
                }],
                session_id: None,
            },
            SessionMessage {
                id: "message-4".to_string(),
                role: "assistant".to_string(),
                timestamp: Some(1_744_366_430_000),
                blocks: vec![ContentBlock {
                    kind: "output_text".to_string(),
                    text: Some("Import finished.".to_string()),
                    tool_name: None,
                    tool_call_id: None,
                    is_error: None,
                    payload: None,
                }],
                session_id: None,
            },
        ],
        events: Vec::new(),
    }
}

fn sample_detail_with_id(
    source_app: SourceApp,
    session_id: &str,
    updated_at: i64,
) -> SessionDetail {
    let mut detail = sample_detail(source_app);
    detail.summary.source_session_id = session_id.to_string();
    detail.summary.transcript_path = format!("/tmp/{session_id}.jsonl");
    detail.summary.updated_at = Some(updated_at);
    detail
}
