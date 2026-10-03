use super::*;

// 用量小时聚合的口径测试:跨天会话按消耗事件的时间戳拆分,Codex 累计值差分,
// 各来源归一后的四项与列表页 family 求和一致;日序列与今日小时序列都由
// 小时桶聚合而来。

struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new() -> Result<Self> {
        let path = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).ok();
    }
}

// 用本地时区的正午构造时间戳,任何时区下都不跨午夜,期望日期即给定日期。
fn local_day_millis(year: i32, month: u32, day: u32) -> i64 {
    local_hour_millis(year, month, day, 12)
}

fn local_hour_millis(year: i32, month: u32, day: u32, hour: u32) -> i64 {
    chrono::TimeZone::with_ymd_and_hms(&chrono::Local, year, month, day, hour, 0, 0)
        .single()
        .expect("valid local timestamp")
        .timestamp_millis()
}

fn rfc3339(timestamp_millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp_millis)
        .expect("valid timestamp")
        .to_rfc3339()
}

fn usage(
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> session::model::SessionTokenUsage {
    session::model::SessionTokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
    }
}

fn today_parts() -> (i32, u32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now();
    (now.year(), now.month(), now.day())
}

#[test]
fn claude_usage_hours_splits_cross_day_sessions_by_message_timestamps() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let session_id = "11111111-1111-4111-8111-111111111111";
    let path = home
        .path
        .join(".claude/projects/demo")
        .join(format!("{session_id}.jsonl"));

    write_jsonl(
        &path,
        &[
            json!({
                "timestamp": rfc3339(local_day_millis(2026, 4, 21)),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "day one answer",
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 20,
                        "cache_read_input_tokens": 1000,
                        "cache_creation_input_tokens": 50
                    }
                }
            }),
            // 同一会话两天后的续聊:按消息时间归到另一天,不并入首日。
            json!({
                "timestamp": rfc3339(local_day_millis(2026, 4, 23)),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "day three answer",
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": 5,
                        "cache_read_input_tokens": 200
                    }
                }
            }),
        ],
    )?;

    let buckets = session::claude_code::usage_hours(&path)?;
    assert_eq!(
        buckets,
        BTreeMap::from([
            ("2026-04-21T12".to_string(), usage(100, 20, 1000, 50)),
            ("2026-04-23T12".to_string(), usage(10, 5, 200, 0)),
        ])
    );

    Ok(())
}

#[test]
fn codex_usage_hours_diffs_cumulative_token_counts_across_days() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let session_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let path = home
        .path
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));

    let token_count_event =
        |timestamp_millis: i64, input: u64, cached: u64, cache_write: u64, output: u64| {
            json!({
                "timestamp": rfc3339(timestamp_millis),
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "total_token_usage": {
                            "input_tokens": input,
                            "cached_input_tokens": cached,
                            "cache_write_input_tokens": cache_write,
                            "output_tokens": output
                        }
                    }
                }
            })
        };

    write_jsonl(
        &path,
        &[
            // 首事件视作从 0 起步:全量归入首日(归一后 600/30/400/100)。
            token_count_event(local_day_millis(2026, 4, 21), 1000, 400, 100, 30),
            // 次日累计到 2000/900/150/60:差分归入次日(500/30/500/50)。
            token_count_event(local_day_millis(2026, 4, 22), 2000, 900, 150, 60),
        ],
    )?;

    let buckets = session::codex::usage_hours(&path)?;
    assert_eq!(
        buckets,
        BTreeMap::from([
            ("2026-04-21T12".to_string(), usage(600, 30, 400, 100)),
            ("2026-04-22T12".to_string(), usage(500, 30, 500, 50)),
        ])
    );

    // 差分求和必须回到最后一条累计值的归一口径,与列表页 summary 一致。
    let total = buckets.values().fold(usage(0, 0, 0, 0), |mut sum, value| {
        sum.accumulate(value);
        sum
    });
    assert_eq!(total, usage(1100, 60, 900, 150));

    Ok(())
}

#[test]
fn codex_usage_hours_clamps_to_zero_when_cumulative_resets() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let path = home
        .path
        .join(".codex/sessions/2026/04/21")
        .join("rollout-2026-04-21T12-00-00-cccccccc-cccc-4ccc-8ccc-cccccccccccc.jsonl");

    let event = |timestamp_millis: i64, input: u64, output: u64| {
        json!({
            "timestamp": rfc3339(timestamp_millis),
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": 0,
                        "cache_write_input_tokens": 0,
                        "output_tokens": output
                    }
                }
            }
        })
    };

    write_jsonl(
        &path,
        &[
            // compact 等重置会让累计值回退;差分钳 0,宁可少计不重复计。
            event(local_day_millis(2026, 4, 21), 1000, 100),
            event(local_day_millis(2026, 4, 22), 300, 20),
        ],
    )?;

    let buckets = session::codex::usage_hours(&path)?;
    assert_eq!(
        buckets,
        BTreeMap::from([
            ("2026-04-21T12".to_string(), usage(1000, 100, 0, 0)),
            ("2026-04-22T12".to_string(), usage(0, 0, 0, 0)),
        ])
    );

    Ok(())
}

#[test]
fn pi_usage_hours_buckets_assistant_and_subagent_runs() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let path = home
        .path
        .join(".pi/agent/sessions/--tmp-pi-usage--")
        .join("2026-09-04T09-00-00-000Z_pi-usage.jsonl");

    write_jsonl(
        &path,
        &[
            json!({
                "type": "session",
                "version": 3,
                "id": "pi-usage",
                "timestamp": rfc3339(local_day_millis(2026, 9, 4)),
                "cwd": "/tmp/pi-usage",
            }),
            json!({
                "type": "message",
                "id": "m1",
                "parentId": null,
                "timestamp": rfc3339(local_day_millis(2026, 9, 4)),
                "message": {
                    "role": "assistant",
                    "content": "answer",
                    "usage": { "input": 197, "output": 180, "cacheRead": 6656, "cacheWrite": 0 }
                }
            }),
            // 子代理 run 的消耗按 toolResult entry 自身的时间戳归桶。
            json!({
                "type": "message",
                "id": "m2",
                "parentId": "m1",
                "timestamp": rfc3339(local_day_millis(2026, 9, 6)),
                "message": {
                    "role": "toolResult",
                    "toolName": "subagent",
                    "toolCallId": "call-1",
                    "content": "subagent report",
                    "details": {
                        "results": [{
                            "title": "run",
                            "usage": { "input": 500, "output": 50, "cacheRead": 200, "cacheWrite": 10 }
                        }]
                    }
                }
            }),
        ],
    )?;

    let buckets = session::pi::usage_hours(&path)?;
    assert_eq!(
        buckets,
        BTreeMap::from([
            ("2026-09-04T12".to_string(), usage(197, 180, 6656, 0)),
            ("2026-09-06T12".to_string(), usage(500, 50, 200, 10)),
        ])
    );

    Ok(())
}

#[test]
fn grokbuild_usage_hours_reads_incremental_turns_by_ended_at() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let dir = home.path.join(".grok/sessions/bucket/session-turns");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("summary.json"),
        serde_json::to_vec(&json!({
            "info": {"id": "session-turns", "cwd": "/synthetic"},
            "chat_format_version": 1,
            "session_summary": "turns",
            "created_at": "2026-09-17T08:00:00Z",
            "updated_at": "2026-09-18T08:00:00Z"
        }))?,
    )?;
    fs::write(
        dir.join("usage.json"),
        serde_json::to_vec(&json!({
            "sessionId": "session-turns",
            "session": {
                "inputTokens": 3000,
                "outputTokens": 90,
                "cachedReadTokens": 1300,
                "cacheCreationTokens": 150
            },
            // turns 是增量切片:sum(turns) == session 汇总。
            "turns": [
                {
                    "turnNumber": 1,
                    "endedAt": rfc3339(local_hour_millis(2026, 9, 17, 10)),
                    "inputTokens": 2000,
                    "outputTokens": 60,
                    "cachedReadTokens": 900,
                    "cacheCreationTokens": 100,
                    "reasoningTokens": 30,
                    "modelCalls": 1,
                    "turnCount": 1
                },
                {
                    "turnNumber": 2,
                    "endedAt": rfc3339(local_day_millis(2026, 9, 18)),
                    "inputTokens": 1000,
                    "outputTokens": 30,
                    "cachedReadTokens": 400,
                    "cacheCreationTokens": 50,
                    "reasoningTokens": 15,
                    "modelCalls": 1,
                    "turnCount": 2
                }
            ]
        }))?,
    )?;

    let summary_path = dir.join("summary.json");
    let buckets = session::grokbuild::usage_hours(&summary_path)?;
    // input 扣掉缓存命中,与 session 汇总口径一致。
    assert_eq!(
        buckets,
        BTreeMap::from([
            ("2026-09-17T10".to_string(), usage(1100, 60, 900, 100)),
            ("2026-09-18T12".to_string(), usage(600, 30, 400, 50)),
        ])
    );

    // 旧版本 usage.json 没有 turns 数组:空桶而不是报错。
    fs::write(
        dir.join("usage.json"),
        serde_json::to_vec(&json!({
            "sessionId": "session-turns",
            "session": {
                "inputTokens": 3000,
                "outputTokens": 90,
                "cachedReadTokens": 1300,
                "cacheCreationTokens": 150
            }
        }))?,
    )?;
    assert!(session::grokbuild::usage_hours(&summary_path)?.is_empty());

    Ok(())
}

#[test]
fn opencode_usage_hours_groups_by_message_time() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let db_path = session::opencode::db_path()?;
    fs::create_dir_all(db_path.parent().unwrap())?;
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
        params!["ses-days", "project-1", "ses-days", "/tmp/root", "Days", "1", 0, 0],
    )?;

    let insert = |id: &str, time: i64, data: &str| {
        connection.execute(
            "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, 'assistant', ?3, ?4, ?4, ?5)",
            params![id, "ses-days", 1, time, data],
        )
    };

    // reasoning 并入输出;data.time.created 优先于 time_created 列
    // (两天的时间都在 data 里,列时间故意错开验证优先级)。
    insert(
        "msg-1",
        local_day_millis(2026, 4, 21),
        &json!({
            "time": { "created": local_day_millis(2026, 4, 21) },
            "tokens": { "input": 1000, "output": 100, "reasoning": 50, "cache": { "read": 500, "write": 20 } }
        })
        .to_string(),
    )?;
    insert(
        "msg-2",
        local_day_millis(2026, 4, 23),
        &json!({
            "time": { "created": local_day_millis(2026, 4, 23) },
            "tokens": { "input": 200, "output": 20, "cache": { "read": 0, "write": 0 } }
        })
        .to_string(),
    )?;
    drop(connection);

    let hours = session::opencode::usage_hours()?.expect("db exists in fixture home");
    assert_eq!(hours.session_count, 1);
    // 夹具日期不是今天,今日会话数为 0。
    assert_eq!(hours.today_session_count, 0);
    assert_eq!(
        hours.buckets,
        BTreeMap::from([
            ("2026-04-21T12".to_string(), usage(1000, 150, 500, 20)),
            ("2026-04-23T12".to_string(), usage(200, 20, 0, 0)),
        ])
    );

    Ok(())
}

#[test]
fn zcode_usage_hours_groups_by_turn_started_at() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let db_path = session::zcode::db_path()?;
    fs::create_dir_all(db_path.parent().unwrap())?;
    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE turn_usage (
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_input_tokens INTEGER NOT NULL DEFAULT 0
        );
        ",
    )?;

    let insert = |turn_id: &str,
                  started_at: i64,
                  input: i64,
                  output: i64,
                  reasoning: i64,
                  creation: i64,
                  cache_read: i64| {
        connection.execute(
            "INSERT INTO turn_usage (session_id, turn_id, started_at, input_tokens, output_tokens, reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["ses-days", turn_id, started_at, input, output, reasoning, creation, cache_read],
        )
    };

    // input_tokens 是全口径,拆掉 cache_read;output 并入 reasoning。
    insert(
        "turn-1",
        local_day_millis(2026, 4, 21),
        1000,
        30,
        10,
        100,
        400,
    )?;
    insert("turn-2", local_day_millis(2026, 4, 23), 200, 5, 0, 0, 0)?;
    drop(connection);

    let hours = session::zcode::usage_hours()?.expect("db exists in fixture home");
    assert_eq!(hours.session_count, 1);
    assert_eq!(hours.today_session_count, 0);
    assert_eq!(
        hours.buckets,
        BTreeMap::from([
            ("2026-04-21T12".to_string(), usage(600, 40, 400, 100)),
            ("2026-04-23T12".to_string(), usage(200, 5, 0, 0)),
        ])
    );

    Ok(())
}

#[test]
fn usage_stats_inner_aggregates_available_sources_and_survives_cache_hits() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let session_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    write_jsonl(
        &home
            .path
            .join(".claude/projects/demo")
            .join(format!("{session_id}.jsonl")),
        &[
            json!({
                "timestamp": rfc3339(local_day_millis(2026, 4, 21)),
                "sessionId": session_id,
                "type": "user",
                "message": { "content": "task" }
            }),
            json!({
                "timestamp": rfc3339(local_day_millis(2026, 4, 21)),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "answer",
                    "usage": { "input_tokens": 100, "output_tokens": 20, "cache_read_input_tokens": 1000 }
                }
            }),
        ],
    )?;

    // 首次为解析结果,第二次走持久缓存;两次结果必须一致。
    let first = session::usage_stats::usage_stats_inner()?;
    let second = session::usage_stats::usage_stats_inner()?;
    assert_eq!(format!("{first:?}"), format!("{second:?}"));

    let apps: Vec<_> = first
        .sources
        .iter()
        .map(|source| source.source_app)
        .collect();
    assert!(
        apps.contains(&SourceApp::ClaudeCode),
        "claude source missing: {apps:?}"
    );
    assert!(
        !apps.contains(&SourceApp::Codex),
        "unavailable source leaked: {apps:?}"
    );

    let claude = first
        .sources
        .iter()
        .find(|source| source.source_app == SourceApp::ClaudeCode)
        .expect("claude stats");
    assert_eq!(claude.session_count, 1);
    assert_eq!(
        claude.days,
        vec![session::model::UsageDayPoint {
            day: "2026-04-21".to_string(),
            usage: usage(100, 20, 1000, 0),
        }]
    );
    // 夹具日期不是今天,今日序列为空。
    assert!(claude.today_hours.is_empty());
    assert_eq!(claude.today_session_count, 0);

    Ok(())
}

#[test]
fn usage_stats_inner_reports_today_hours_and_today_sessions() -> Result<()> {
    let home = TempHome::new()?;
    let _guard = TestEnvGuard::set_home(&home.path);

    let (year, month, day) = today_parts();
    let session_id = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
    write_jsonl(
        &home
            .path
            .join(".claude/projects/demo")
            .join(format!("{session_id}.jsonl")),
        &[
            json!({
                "timestamp": rfc3339(local_hour_millis(year, month, day, 10)),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "morning",
                    "usage": { "input_tokens": 30, "output_tokens": 3 }
                }
            }),
            json!({
                "timestamp": rfc3339(local_hour_millis(year, month, day, 14)),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "afternoon",
                    "usage": { "input_tokens": 5, "output_tokens": 7 }
                }
            }),
            // 昨天的消耗不进今日序列。
            json!({
                "timestamp": rfc3339(local_day_millis(year, month, day) - 86_400_000),
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "yesterday",
                    "usage": { "input_tokens": 999, "output_tokens": 999 }
                }
            }),
        ],
    )?;

    let stats = session::usage_stats::usage_stats_inner()?;
    let claude = stats
        .sources
        .iter()
        .find(|source| source.source_app == SourceApp::ClaudeCode)
        .expect("claude stats");

    // 同一天的两个小时各自成点,昨天的量只进日序列。
    assert_eq!(
        claude.today_hours,
        vec![
            session::model::UsageHourPoint {
                hour: 10,
                usage: usage(30, 3, 0, 0)
            },
            session::model::UsageHourPoint {
                hour: 14,
                usage: usage(5, 7, 0, 0)
            },
        ]
    );
    assert_eq!(claude.today_session_count, 1);

    let today_key = format!("{year:04}-{month:02}-{day:02}");
    let today_point = claude
        .days
        .iter()
        .find(|point| point.day == today_key)
        .expect("today in day series");
    assert_eq!(today_point.usage, usage(35, 10, 0, 0));

    Ok(())
}
