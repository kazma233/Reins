use super::*;

// zcode fixture:与真实库同构的最小 schema,只建 reader 查询用到的列。
// 数据形态对应 ~/.zcode/cli/db/db.sqlite 的实测结构,见
// aidocs/context/2026-09-29-zcode-session-storage-exploration.md。
struct ZcodeSeed {
    root_id: String,
    child_id: String,
}

fn seed_zcode_family(root_id: &str, child_id: &str) -> Result<ZcodeSeed> {
    let db_path = session::zcode::db_path()?;

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            parent_id TEXT,
            directory TEXT NOT NULL,
            title TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            sequence INTEGER,
            data TEXT NOT NULL
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            sequence INTEGER,
            data TEXT NOT NULL
        );
        CREATE TABLE turn_usage (
            session_id TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_input_tokens INTEGER NOT NULL DEFAULT 0
        );
        ",
    )?;

    connection.execute(
        "INSERT INTO session (id, parent_id, directory, title, time_created, time_updated) VALUES (?1, NULL, ?2, ?3, 1000, 5000)",
        params![root_id, "/tmp/root", "Root task"],
    )?;
    connection.execute(
        "INSERT INTO session (id, parent_id, directory, title, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, 2000, 3000)",
        params![child_id, root_id, "/tmp/root", "Child task"],
    )?;

    let insert_message = |id: &str, session_id: &str, time_created: i64, sequence: i64, data: Value| {
        connection.execute(
            "INSERT INTO message (id, session_id, time_created, sequence, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, session_id, time_created, sequence, data.to_string()],
        )
    };
    let insert_part = |id: &str, message_id: &str, session_id: &str, time_created: i64, sequence: i64, data: Value| {
        connection.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, sequence, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, message_id, session_id, time_created, sequence, data.to_string()],
        )
    };

    // root 的用户输入:text + file 附件
    insert_message(
        "msg_user",
        root_id,
        1100,
        0,
        json!({
            "role": "user",
            "time": { "created": 1100 },
            "semantics": { "kind": "user_prompt" },
        }),
    )?;
    insert_part("part_user_text", "msg_user", root_id, 1100, 0, json!({
        "type": "text",
        "text": "帮我看下这个项目",
    }))?;
    insert_part("part_user_file", "msg_user", root_id, 1100, 1, json!({
        "type": "file",
        "mime": "image/png",
        "url": "zcode-artifact://sess/file-1",
    }))?;

    // root 的助手回复:reasoning + text + tool
    insert_message(
        "msg_assistant",
        root_id,
        1200,
        1,
        json!({
            "role": "assistant",
            "time": { "created": 1200 },
            "semantics": { "kind": "assistant_response" },
            "modelId": "GLM-5.3",
        }),
    )?;
    insert_part("part_reasoning", "msg_assistant", root_id, 1201, 0, json!({
        "type": "reasoning",
        "text": "先查看目录结构",
    }))?;
    insert_part("part_text", "msg_assistant", root_id, 1202, 1, json!({
        "type": "text",
        "text": "目录里有 src 和 README",
    }))?;
    insert_part("part_tool", "msg_assistant", root_id, 1203, 2, json!({
        "type": "tool",
        "callID": "call_1",
        "tool": "Bash",
        "state": {
            "status": "completed",
            "input": { "command": "ls" },
            "output": "README.md\nsrc",
        },
    }))?;
    // step-start/step-finish 是统计噪音,断言它们不进时间线
    insert_part("part_step_start", "msg_assistant", root_id, 1200, 3, json!({
        "type": "step-start",
    }))?;
    insert_part("part_step_finish", "msg_assistant", root_id, 1299, 4, json!({
        "type": "step-finish",
        "reason": "tool-calls",
    }))?;

    // root 的系统注入消息:todo_reminder 归事件
    insert_message(
        "msg_todo",
        root_id,
        1300,
        2,
        json!({
            "role": "user",
            "time": { "created": 1300 },
            "semantics": { "kind": "todo_reminder" },
        }),
    )?;
    insert_part("part_todo_text", "msg_todo", root_id, 1300, 0, json!({
        "type": "text",
        "text": "- [ ] 分析存储\n- [ ] 写 reader",
    }))?;

    // root 的 timeline_event 消息(如 model_change 分隔线)
    insert_message(
        "msg_timeline",
        root_id,
        1400,
        3,
        json!({
            "role": "assistant",
            "semantics": { "kind": "timeline_event" },
        }),
    )?;
    insert_part("part_timeline", "msg_timeline", root_id, 1400, 0, json!({
        "type": "timeline",
        "timelineType": "model_change",
    }))?;

    // child(subagent)的助手回复
    insert_message(
        "msg_child",
        child_id,
        2100,
        0,
        json!({
            "role": "assistant",
            "time": { "created": 2100 },
            "semantics": { "kind": "assistant_response" },
        }),
    )?;
    insert_part("part_child_text", "msg_child", child_id, 2100, 0, json!({
        "type": "text",
        "text": "子任务完成",
    }))?;

    // turn_usage:root 与 child 各一条,family 用量应为两者之和
    connection.execute(
        "INSERT INTO turn_usage (session_id, input_tokens, output_tokens, reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens) VALUES (?1, 100, 20, 5, 10, 50)",
        params![root_id],
    )?;
    connection.execute(
        "INSERT INTO turn_usage (session_id, input_tokens, output_tokens, reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens) VALUES (?1, 30, 8, 2, 4, 15)",
        params![child_id],
    )?;

    Ok(ZcodeSeed {
        root_id: root_id.to_string(),
        child_id: child_id.to_string(),
    })
}

fn zcode_test_home() -> Result<(PathBuf, TestEnvGuard)> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let guard = TestEnvGuard::set_home(&temp_home);
    Ok((temp_home, guard))
}

#[test]
fn zcode_root_session_aggregates_subagent_sessions() -> Result<()> {
    let (temp_home, _guard) = zcode_test_home()?;
    let seed = seed_zcode_family("sess_root", "sess_child")?;

    let root_path = session::zcode::session_path(&seed.root_id);
    let reader = session::reader(SourceApp::Zcode);

    let summary = reader.parse_summary(&root_path)?;
    assert_eq!(summary.source_session_id, seed.root_id);
    assert!(summary.title.contains("Root task (+1 subagents)"));
    assert_eq!(summary.cwd.as_deref(), Some("/tmp/root"));
    // turn_usage 按 family 求和:input 100+30,output (20+5)+(8+2),cache 同理
    let usage = summary.token_usage.expect("token usage present");
    assert_eq!(usage.input_tokens, 130);
    assert_eq!(usage.output_tokens, 35);
    assert_eq!(usage.cache_read_tokens, 65);
    assert_eq!(usage.cache_write_tokens, 14);

    let detail = read_detail(reader, &root_path)?;

    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| {
                block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("Sub-agent session: Child task"))
            }))
    );
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "subagent_started")
    );
    assert!(
        detail
            .source_paths
            .iter()
            .any(|path| path.ends_with(&format!(":{}", seed.child_id)))
    );

    // 子代理入口按 agent session id 取子会话消息
    let agent_messages = reader.parse_agent_messages(&root_path, &seed.child_id)?;
    assert!(agent_messages
        .iter()
        .any(|message| message.blocks.iter().any(|block| block
            .text
            .as_deref()
            .is_some_and(|text| text.contains("子任务完成")))));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn zcode_overview_counts_match_loaded_timeline() -> Result<()> {
    let (temp_home, _guard) = zcode_test_home()?;
    let seed = seed_zcode_family("sess_root", "sess_child")?;

    let root_path = session::zcode::session_path(&seed.root_id);
    let reader = session::reader(SourceApp::Zcode);
    let overview = reader.parse_overview(&root_path)?;
    let detail = read_detail(reader, &root_path)?;

    assert_eq!(overview.message_count, Some(detail.messages.len()));
    assert_eq!(overview.event_count, Some(detail.events.len()));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn zcode_classifies_timeline_by_semantics_and_parses_parts() -> Result<()> {
    let (temp_home, _guard) = zcode_test_home()?;
    let seed = seed_zcode_family("sess_root", "sess_child")?;

    let root_path = session::zcode::session_path(&seed.root_id);
    let detail = read_detail(session::reader(SourceApp::Zcode), &root_path)?;

    // user_prompt:text 与 file 附件都可见
    let user_message = detail
        .messages
        .iter()
        .find(|message| message.id == "msg_user")
        .expect("user prompt message is a timeline message");
    assert_eq!(user_message.role, "user");
    assert!(user_message.blocks.iter().any(|block| block.kind == "text"
        && block.text.as_deref() == Some("帮我看下这个项目")));
    assert!(user_message
        .blocks
        .iter()
        .any(|block| block.kind == "file" && block.text.as_deref() == Some("文件类型:image/png")));

    // assistant_response:reasoning→thinking、text、tool 拆两条 UI 块;
    // step-start/step-finish 不产生任何块
    let assistant = detail
        .messages
        .iter()
        .find(|message| message.id == "msg_assistant")
        .expect("assistant response message is a timeline message");
    assert_eq!(assistant.role, "assistant");
    assert!(assistant
        .blocks
        .iter()
        .any(|block| block.kind == "thinking" && block.text.as_deref() == Some("先查看目录结构")));
    let tool_input = assistant
        .blocks
        .iter()
        .find(|block| block.kind == "function_call")
        .expect("tool call block");
    assert_eq!(tool_input.tool_name.as_deref(), Some("Bash"));
    assert_eq!(tool_input.tool_call_id.as_deref(), Some("call_1"));
    assert!(tool_input
        .payload
        .as_ref()
        .and_then(|payload| payload.get("input"))
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|command| command == "ls"));
    let tool_output = assistant
        .blocks
        .iter()
        .find(|block| block.kind == "function_call_output")
        .expect("tool output block");
    assert_eq!(tool_output.text.as_deref(), Some("README.md\nsrc"));
    assert_eq!(
        tool_output
            .payload
            .as_ref()
            .and_then(|payload| payload.get("output"))
            .and_then(Value::as_str),
        Some("README.md\nsrc")
    );

    // todo_reminder / timeline_event 不进消息,转事件
    assert!(!detail
        .messages
        .iter()
        .any(|message| matches!(message.id.as_str(), "msg_todo" | "msg_timeline")));
    let todo_event = detail
        .events
        .iter()
        .find(|event| event.id == "msg_todo")
        .expect("todo reminder becomes an event");
    assert!(todo_event.summary.contains("todo reminder"));
    assert!(todo_event
        .payload
        .as_ref()
        .and_then(|payload| payload.get("text"))
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("分析存储")));
    let timeline_event = detail
        .events
        .iter()
        .find(|event| event.id == "msg_timeline")
        .expect("timeline event message becomes an event");
    assert!(timeline_event.summary.contains("model_change"));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
