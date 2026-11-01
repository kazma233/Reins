//! 转录存储状态:压缩形态与归档状态的读取与角标来源。
//!
//! 每个用例对应一个上游真实契约:
//! - codex 冷会话压缩(`.jsonl.zst`,上游 rollout/src/compression.rs)、
//!   归档会话移动到 `archived_sessions/`(thread-store/local/archive_thread.rs);
//! - dsh 的 v0 文件名(`session.jsonl[.zstd]` 不带版本段)与
//!   `storages/workspace.json` 的归档集;
//! - zcode/opencode 的 `time_archived` 列(老库可能没有该列)。
//!
//! 夹具里的 zstd 字节由 Node zlib.zstdCompressSync 生成(codex 单帧、dsh 两帧),
//! 与上游写出的形态一致;明文内容见各用例断言。

use super::*;
use crate::session::SessionReader;
use crate::tests::zcode::seed_zcode_family_at;

// codex 冷会话的压缩转录:整文件单帧 zstd,解出即下面断言里的那几行 JSONL。
pub(super) const CODEX_ROLLOUT_ZST: &[u8] = &[
    0x28, 0xb5, 0x2f, 0xfd, 0x60, 0x59, 0x02, 0xa5, 0x0a, 0x00, 0xf6, 0x52, 0x39, 0x1f, 0x60, 0x69,
    0xf2, 0x06, 0xf3, 0x18, 0x2d, 0xa9, 0x6e, 0xda, 0xca, 0xc6, 0x7f, 0x56, 0x5b, 0x3d, 0x12, 0x00,
    0x8e, 0xc1, 0xf3, 0xe8, 0xde, 0xe2, 0x81, 0x15, 0x09, 0x61, 0xd0, 0xc1, 0x3c, 0x30, 0x00, 0x30,
    0x00, 0x2f, 0x00, 0x68, 0xd3, 0x36, 0xb7, 0x4b, 0xda, 0xb6, 0xfa, 0x2d, 0x2c, 0xb9, 0xc8, 0xae,
    0x67, 0xa8, 0x62, 0x3b, 0x77, 0xf6, 0x4d, 0xcb, 0x72, 0xe1, 0x17, 0x59, 0x82, 0x73, 0x0e, 0xc0,
    0x49, 0x07, 0x0d, 0xa0, 0x63, 0x0e, 0xc7, 0x04, 0x28, 0x47, 0xd9, 0xb4, 0x12, 0xae, 0x77, 0x86,
    0xee, 0x77, 0x09, 0xef, 0x14, 0x7b, 0x6c, 0xe9, 0x3c, 0x95, 0x06, 0xb7, 0x66, 0x26, 0x61, 0x5e,
    0x2e, 0x36, 0xed, 0xe3, 0xd6, 0xf0, 0x1b, 0xfc, 0xb1, 0x99, 0x8a, 0x93, 0x3e, 0x5a, 0x0c, 0xb5,
    0xb6, 0xd7, 0x2c, 0xf1, 0xe8, 0x78, 0x26, 0x12, 0x7e, 0xc2, 0x60, 0x8b, 0xac, 0x80, 0x41, 0x08,
    0x21, 0x8c, 0x09, 0xcd, 0x0e, 0xd8, 0x53, 0x71, 0x4c, 0x66, 0x78, 0x5c, 0x4f, 0xfb, 0xe3, 0x7a,
    0x77, 0xee, 0xeb, 0x60, 0x50, 0x37, 0xb6, 0x7b, 0x67, 0x9a, 0x70, 0x1c, 0x9b, 0xd4, 0x53, 0x19,
    0x5d, 0x41, 0xda, 0x8f, 0x93, 0x9d, 0x6e, 0x5a, 0x7e, 0x3d, 0x17, 0x59, 0x5e, 0x2c, 0x77, 0x86,
    0x90, 0x03, 0x86, 0x5a, 0x6b, 0x3d, 0x88, 0xdf, 0x4d, 0x86, 0x4a, 0x28, 0xaa, 0xf1, 0x01, 0x5b,
    0x6c, 0x91, 0x73, 0x0e, 0x4a, 0xeb, 0x4b, 0xa2, 0xea, 0xad, 0xef, 0x6f, 0x45, 0x74, 0x94, 0x11,
    0xfa, 0x17, 0x0e, 0xd5, 0x47, 0x56, 0x60, 0xc4, 0xd3, 0x7e, 0xaf, 0xe7, 0xd1, 0xd8, 0x9d, 0xa7,
    0x84, 0x01, 0x2c, 0x20, 0x50, 0x82, 0x31, 0x54, 0x93, 0x37, 0x55, 0xbd, 0xa0, 0xc8, 0x49, 0x73,
    0x8c, 0x47, 0xd0, 0x0f, 0x2e, 0x87, 0x2c, 0x2d, 0x03, 0x66, 0x3d, 0x34, 0x7c, 0xc1, 0xde, 0x0c,
    0xdc, 0x54, 0xe4, 0x8a, 0x63, 0xea, 0xb3, 0xb2, 0x59, 0xbf, 0xab, 0xbb, 0xf6, 0xfd, 0xb8, 0xda,
    0xde, 0xa0, 0x5d, 0xb9, 0x23, 0x5d, 0xaf, 0x49, 0x02, 0xca, 0x2f, 0x09, 0x2d, 0x54, 0x22, 0xbc,
    0x96, 0x8b, 0xb3, 0x34, 0x73, 0x84, 0x0e, 0x06, 0x36, 0x24, 0x03, 0x36, 0x28, 0x3f, 0x74, 0x6c,
    0x12, 0xa4, 0x55, 0x52, 0x8e, 0x7a, 0x75, 0x10, 0xcd, 0x86, 0x3e, 0x86, 0x00, 0x14, 0x5b, 0x21,
    0xf7, 0xbd, 0x6c, 0xf1, 0x03, 0x74, 0x2d, 0xb1, 0xee, 0x19, 0x2f, 0xb9, 0xf2, 0x3c,
];

// dsh 的 v0 转录:两帧拼接(帧1=header,帧2=user+assistant),对应上游
// generation 格式的 v0 文件名 `session.jsonl.zstd`(不带 .v{N} 段)。
const DSH_V0_ZST: &[u8] = &[
    0x28, 0xb5, 0x2f, 0xfd, 0x20, 0xa6, 0x15, 0x04, 0x00, 0x22, 0xc8, 0x1a, 0x1d, 0x60, 0x45, 0xd5,
    0x01, 0xc3, 0x26, 0x45, 0x9a, 0x3f, 0xf7, 0x7e, 0xfd, 0x3a, 0x9a, 0x47, 0x47, 0x05, 0x16, 0x74,
    0x82, 0xf2, 0x90, 0xf6, 0xf6, 0x20, 0x09, 0xb2, 0x04, 0x4f, 0x81, 0x00, 0x67, 0xc3, 0x64, 0xb8,
    0x7b, 0x7d, 0x43, 0x2c, 0x3b, 0x83, 0xcd, 0x46, 0x3f, 0x30, 0x68, 0xed, 0xd3, 0x77, 0x75, 0x50,
    0xa1, 0x9c, 0xcf, 0x3f, 0xa1, 0xcb, 0xe7, 0x69, 0x41, 0xe6, 0xce, 0xd1, 0x68, 0x1e, 0xd1, 0xb3,
    0xc7, 0x62, 0xf1, 0x21, 0x03, 0x45, 0x11, 0x24, 0x05, 0x42, 0xa2, 0xbc, 0x86, 0xfc, 0xe2, 0xdb,
    0xe2, 0x33, 0x22, 0x19, 0xd0, 0xf3, 0x34, 0x72, 0xf9, 0x10, 0xd5, 0x2e, 0xf9, 0xcc, 0xa0, 0xa5,
    0x7a, 0xb5, 0xf2, 0xa3, 0x69, 0x0d, 0x10, 0x07, 0x00, 0x7a, 0x84, 0x8f, 0xc3, 0x9d, 0xfe, 0xb4,
    0x57, 0x95, 0x91, 0x71, 0xd6, 0xf9, 0xdc, 0xa2, 0x6d, 0x69, 0x02, 0x28, 0xb5, 0x2f, 0xfd, 0x60,
    0x38, 0x01, 0xc5, 0x08, 0x00, 0xc6, 0x50, 0x33, 0x21, 0x40, 0xa9, 0x54, 0x07, 0x76, 0x1b, 0xd1,
    0x16, 0x51, 0xcb, 0x0d, 0x61, 0x06, 0xe9, 0xd8, 0x50, 0xc5, 0x80, 0xeb, 0x57, 0x71, 0x60, 0x47,
    0xfc, 0x4f, 0xf4, 0xed, 0xca, 0x29, 0x0a, 0x48, 0x00, 0x08, 0x29, 0x00, 0x29, 0x00, 0x28, 0x00,
    0x2e, 0x9d, 0xec, 0x06, 0xae, 0xb2, 0xf1, 0xa0, 0xfa, 0x20, 0x45, 0x51, 0x94, 0x80, 0xc5, 0x42,
    0x81, 0x04, 0x18, 0x28, 0xf9, 0x91, 0xb5, 0x3e, 0x28, 0x59, 0x72, 0xf5, 0xf1, 0x1d, 0xa9, 0xfa,
    0x08, 0xc0, 0xbb, 0x42, 0x27, 0xdf, 0x2d, 0xac, 0x9b, 0x5e, 0xd8, 0x82, 0x5c, 0x3d, 0xd8, 0xd4,
    0x07, 0x21, 0x9a, 0xb5, 0xb5, 0x7a, 0x28, 0x39, 0x28, 0x93, 0xc3, 0x82, 0x4a, 0xc7, 0x19, 0xa7,
    0xe3, 0x55, 0xf8, 0x1e, 0x24, 0x2f, 0xfb, 0xb2, 0xa6, 0xdc, 0x24, 0xe7, 0xe6, 0xe2, 0xe3, 0x85,
    0xdd, 0x05, 0xff, 0xe2, 0xe4, 0xf7, 0x65, 0x4d, 0xda, 0x73, 0x3e, 0x66, 0xbe, 0xb4, 0xc8, 0x7b,
    0x82, 0xb2, 0x3c, 0x6e, 0x92, 0xfb, 0x2a, 0x7b, 0x70, 0x13, 0x89, 0x65, 0xb9, 0xba, 0x54, 0xd3,
    0x10, 0xa3, 0x11, 0x62, 0xf9, 0x98, 0x69, 0xcf, 0x27, 0x06, 0x86, 0x18, 0xe3, 0xe2, 0xbb, 0x9e,
    0x81, 0x62, 0xc9, 0x13, 0xfc, 0xa1, 0x52, 0xf9, 0x30, 0x16, 0x36, 0x1f, 0x84, 0x92, 0x95, 0x1d,
    0xd2, 0x0a, 0x16, 0x3a, 0xb6, 0xdc, 0x24, 0x98, 0x40, 0x44, 0x3f, 0x34, 0x99, 0x06, 0x6c, 0x4a,
    0x25, 0x02, 0xf2, 0xab, 0x0b, 0x1c, 0x00, 0x38, 0x89, 0xde, 0xcc, 0x2c, 0x29, 0x42, 0xd7, 0x98,
    0xa8, 0xa6, 0x27, 0x6e, 0x2c, 0x6e, 0xbe, 0x04, 0x90, 0x95, 0xcd, 0xa7, 0x81, 0xa4, 0x03, 0x02,
    0xc7, 0xf5, 0xec, 0xa6, 0x94, 0xf0, 0x66, 0xb0, 0x0e, 0x1d, 0x28, 0x46, 0x30, 0x0b, 0x14, 0x58,
    0x47, 0x06, 0x56, 0x4b, 0xa0, 0x39, 0xa0, 0x19, 0xd2, 0x90, 0x3c, 0x43, 0x81, 0x3e, 0x24, 0x93,
    0x9e, 0x98, 0x8a, 0x25, 0xb9, 0x08, 0xd6, 0xa9, 0x46, 0x71, 0x5f, 0x11, 0xcc,
];

pub(super) const CODEX_COLD_ID: &str = "11111111-1111-4111-8111-111111111111";

fn temp_store() -> PathBuf {
    env::temp_dir().join(format!("reins-store-{}", Uuid::new_v4()))
}

fn codex_home(label: &str) -> PathBuf {
    env::temp_dir().join(format!("reins-codex-{label}-{}", Uuid::new_v4()))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

// 冷会话 rollout 的明文内容,压缩夹具解出来就是它。
pub(super) fn cold_rollout_lines(rollout_id: &str) -> Vec<Value> {
    vec![
        json!({
            "timestamp": "2026-03-01T10:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": rollout_id,
                "cwd": "/tmp/reins/cold",
                "timestamp": "2026-03-01T10:00:00.000Z"
            }
        }),
        json!({
            "timestamp": "2026-03-01T10:00:01.000Z",
            "type": "turn_context",
            "payload": { "model": "gpt-5-codex" }
        }),
        json!({
            "timestamp": "2026-03-01T10:00:02.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "cold session question" }]
            }
        }),
        json!({
            "timestamp": "2026-03-01T10:00:03.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "answer from compressed rollout" }]
            }
        }),
        json!({
            "timestamp": "2026-03-01T10:00:04.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": 1000,
                        "cached_input_tokens": 200,
                        "output_tokens": 50,
                        "cache_write_input_tokens": 10
                    }
                }
            }
        }),
    ]
}

#[test]
fn codex_reads_compressed_rollout_and_flags_it() -> Result<()> {
    let home = codex_home("zst");
    let path = home.join("sessions/2026/03/01").join(format!(
        "rollout-2026-03-01T10-00-00-{CODEX_COLD_ID}.jsonl.zst"
    ));
    write_bytes(&path, CODEX_ROLLOUT_ZST)?;

    let reader = session::codex::engine_at(home, temp_store());
    let summary = reader.parse_summary(CODEX_COLD_ID)?;

    assert_eq!(summary.title, "cold session question");
    assert!(summary.compressed, "压缩形态必须置位");
    assert!(!summary.archived);
    // usage 归一口径:input 扣掉缓存命中,缓存读写单独计数
    let usage = summary.token_usage.expect("compressed rollout usage");
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens
        ),
        (800, 50, 200, 10)
    );

    // 时间线也要能从同一份压缩转录里读出来
    let detail = read_detail(&reader, CODEX_COLD_ID)?;
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("answer from compressed rollout"))
    }));

    Ok(())
}

#[test]
fn codex_lists_archived_rollouts_in_the_same_list() -> Result<()> {
    let home = codex_home("archived");
    let archived_id = "22222222-2222-4222-8222-222222222222";
    write_jsonl(
        &home
            .join("archived_sessions")
            .join(format!("rollout-2026-03-01T11-00-00-{archived_id}.jsonl")),
        &cold_rollout_lines(archived_id),
    )?;

    let reader = session::codex::engine_at(home, temp_store());
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1, "归档会话仍属于用户的会话历史");

    let summary = reader.parse_summary(archived_id)?;
    assert!(summary.archived, "archived_sessions 下的会话必须置归档位");
    assert!(!summary.compressed);

    Ok(())
}

#[test]
fn codex_keeps_one_row_when_plain_and_compressed_siblings_coexist() -> Result<()> {
    let home = codex_home("siblings");
    let dir = home.join("sessions/2026/03/01");
    let plain = dir.join(format!("rollout-2026-03-01T12-00-00-{CODEX_COLD_ID}.jsonl"));
    write_jsonl(&plain, &cold_rollout_lines(CODEX_COLD_ID))?;
    write_bytes(
        &plain.with_file_name(format!(
            "rollout-2026-03-01T12-00-00-{CODEX_COLD_ID}.jsonl.zst"
        )),
        CODEX_ROLLOUT_ZST,
    )?;

    let reader = session::codex::engine_at(home, temp_store());
    let entries = reader.list_entries()?;

    // 上游读取侧明文优先(should_skip_compressed_sibling),不能出两条行。
    assert_eq!(entries.len(), 1);
    let summary = reader.parse_summary(CODEX_COLD_ID)?;
    assert!(!summary.compressed, "两份共存时读的是明文");

    Ok(())
}

#[test]
fn codex_keeps_reading_a_rollout_compressed_after_the_catalog_was_built() -> Result<()> {
    let home = codex_home("in-place");
    let dir = home.join("sessions/2026/03/01");
    let plain = dir.join(format!("rollout-2026-03-01T13-00-00-{CODEX_COLD_ID}.jsonl"));
    write_jsonl(&plain, &cold_rollout_lines(CODEX_COLD_ID))?;

    let reader = session::codex::engine_at(home.clone(), temp_store());
    let before = reader.parse_summary(CODEX_COLD_ID)?;
    assert!(!before.compressed);

    // 模拟 codex 后台 worker:原地压成 .jsonl.zst 并删除明文;索引必须重建,
    // 旧明文路径经 resolve 归一后仍可读。
    fs::remove_file(&plain)?;
    let compressed = plain.with_file_name(format!(
        "rollout-2026-03-01T13-00-00-{CODEX_COLD_ID}.jsonl.zst"
    ));
    write_bytes(&compressed, CODEX_ROLLOUT_ZST)?;
    push_mtime_forward(&compressed)?;

    let after = reader.parse_summary(CODEX_COLD_ID)?;
    assert!(after.compressed, "压缩后索引与角标都要跟上");
    assert_eq!(after.title, "cold session question");

    Ok(())
}

// 索引失效用的是毫秒精度 mtime,而夹具写入发生在同一毫秒内:显式把文件时刻
// 往前推一段,模拟"这件事发生在之后的某一刻"。
fn push_mtime_forward(path: &Path) -> Result<()> {
    let ahead = std::time::SystemTime::now() + std::time::Duration::from_secs(1);
    fs::File::options()
        .write(true)
        .open(path)?
        .set_times(fs::FileTimes::new().set_modified(ahead))?;
    Ok(())
}

#[test]
fn dsh_reads_v0_transcripts_without_version_segment() -> Result<()> {
    let sessions = env::temp_dir().join(format!("reins-dsh-v0-{}", Uuid::new_v4()));
    let project = sessions.join("--D-projects-demo--");

    // v0 的规范文件名是 session.jsonl[.zstd]:既没有 .v0,也不能因为缺版本段
    // 被当成非转录文件。
    let compressed = project.join("session-demo-v0").join("session.jsonl.zstd");
    write_bytes(&compressed, DSH_V0_ZST)?;
    let plain = project.join("session-demo-v0-plain").join("session.jsonl");
    write_jsonl(
        &plain,
        &[
            json!({
                "type": "session",
                "version": 0,
                "id": "session-demo-v0-plain",
                "createdAt": 1791355600000_i64,
                "cwd": "/tmp/dsh/legacy"
            }),
            json!({
                "type": "user/message",
                "seq": 1,
                "time": 1791355600001_i64,
                "surfaceOp": "append",
                "data": {
                    "content": [{ "type": "text", "text": "plain v0 question" }],
                    "source": { "kind": "user" },
                    "role": "user",
                    "id": "user-1"
                }
            }),
        ],
    )?;

    let reader = session::dsh::engine_at(sessions, temp_store());
    assert_eq!(reader.list_entries()?.len(), 2, "v0 的两个形态都要出现");

    let compressed_summary = reader.parse_summary("session-demo-v0")?;
    assert_eq!(compressed_summary.title, "legacy v0 question");
    let usage = compressed_summary.token_usage.expect("v0 usage");
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens
        ),
        (11, 7, 3, 5)
    );
    assert_eq!(
        reader.parse_summary("session-demo-v0-plain")?.title,
        "plain v0 question"
    );

    Ok(())
}

#[test]
fn dsh_flags_archived_sessions_from_workspace_registry() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-dsh-archive-{}", Uuid::new_v4()));
    let sessions = root.join("sessions");
    let project = sessions.join("--D-projects-demo--");

    let archived_id = "session-archived-one";
    let active_id = "session-active-one";
    for id in [archived_id, active_id] {
        write_jsonl(
            &project.join(id).join("session.v4.jsonl"),
            &[
                json!({
                    "type": "session",
                    "version": 4,
                    "id": id,
                    "createdAt": 1791355600000_i64,
                    "cwd": "/tmp/dsh/workspace"
                }),
                json!({
                    "type": "user/message",
                    "seq": 1,
                    "time": 1791355600001_i64,
                    "surfaceOp": "append",
                    "data": {
                        "content": [{ "type": "text", "text": "task" }],
                        "role": "user",
                        "id": "user-1"
                    }
                }),
            ],
        )?;
    }

    let registry = root.join("storages/workspace.json");
    write_jsonl(
        &registry,
        &[json!({
            "global": { "archivedSessionIds": [archived_id] }
        })],
    )?;

    let reader = session::dsh::engine_at(sessions, temp_store());
    assert!(reader.parse_summary(archived_id)?.archived);
    assert!(!reader.parse_summary(active_id)?.archived);

    // 归档只改账本不改转录,索引指纹必须带上账本时刻,否则角标不会更新。
    write_jsonl(
        &registry,
        &[json!({
            "global": { "archivedSessionIds": [archived_id, active_id] }
        })],
    )?;
    push_mtime_forward(&registry)?;
    assert!(reader.parse_summary(active_id)?.archived);

    Ok(())
}

#[test]
fn zcode_flags_archived_sessions_from_time_archived() -> Result<()> {
    let db_dir = env::temp_dir().join(format!("reins-zcode-archived-{}", Uuid::new_v4()));
    fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("db.sqlite");

    seed_zcode_family_at(&db_path, "root-archived", "child-active")?;
    let connection = Connection::open(&db_path)?;
    connection.execute("ALTER TABLE session ADD COLUMN time_archived INTEGER", [])?;
    connection.execute(
        "UPDATE session SET time_archived = 5000 WHERE id = 'root-archived'",
        [],
    )?;
    // 对照组必须是独立会话(非归档会话的子会话会被并进同一 family,角标取自
    // family root,断言不出未归档态)。
    connection.execute(
        "INSERT INTO session (id, parent_id, directory, title, time_created, time_updated, time_archived) VALUES ('standalone', NULL, '/tmp/root', 'Standalone task', 1000, 2000, NULL)",
        [],
    )?;
    drop(connection);

    let reader = session::zcode::engine_at(db_path, temp_store());
    assert!(reader.parse_summary("root-archived")?.archived);
    assert!(!reader.parse_summary("standalone")?.archived);

    Ok(())
}

#[test]
fn opencode_flags_archived_sessions_from_time_archived() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-opencode-archived-{}", Uuid::new_v4()));
    seed_opencode_family_at(&root, "ses_archived_root", "ses_active_child")?;

    let db_path = root.join("opencode.db");
    let connection = Connection::open(&db_path)?;
    connection.execute(
        "ALTER TABLE session_v2 ADD COLUMN time_archived INTEGER",
        [],
    )?;
    connection.execute(
        "UPDATE session_v2 SET time_archived = 5000 WHERE id = 'ses_archived_root'",
        [],
    )?;
    connection.execute(
        "INSERT INTO session_v2 (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated, time_archived) VALUES ('ses_standalone', 'project-1', NULL, 'ses_standalone', '/tmp/root', 'Standalone task', '1', 1744366400000, 1744366500000, NULL)",
        [],
    )?;
    drop(connection);

    let reader = session::opencode::engine_at(root, temp_store());
    assert!(reader.parse_summary("ses_archived_root")?.archived);
    assert!(!reader.parse_summary("ses_standalone")?.archived);

    Ok(())
}


