use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ContentBlock, SessionDetail, SessionEvent, SessionEventPage, SessionFileEntry, SessionMessage,
    SessionMessagePage, SessionOverview, SessionReader, SessionSummary, SourceApp,
};

use super::family_index::{Family, FamilyIndex, FamilyRow};
use super::family_timeline::{FamilyAgentLabel, family_agents};

pub(crate) struct GrokBuildBackend;
pub(crate) static BACKEND: GrokBuildBackend = GrokBuildBackend;

pub(crate) fn root() -> Result<PathBuf> {
    crate::support::fs::grok_home_path(
        std::env::var_os("GROK_HOME"),
        crate::support::fs::user_home_dir(),
    )
    .map(|home| home.join("sessions"))
    .context("Unable to determine Grok Build home")
}

#[derive(Deserialize)]
struct Info {
    id: String,
    cwd: String,
}
#[derive(Deserialize)]
struct Summary {
    info: Info,
    chat_format_version: u64,
    session_summary: String,
    generated_title: Option<String>,
    created_at: String,
    updated_at: String,
    attempt_id: Option<String>,
    session_kind: Option<String>,
}

#[derive(Clone, Deserialize)]
struct SubagentMeta {
    subagent_id: String,
    attempt_id: String,
    parent_session_id: String,
    child_session_id: String,
    subagent_type: Option<String>,
    description: Option<String>,
    status: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    // 声明关系时子会话应当存在的同级目录路径，仅用于缺失子会话的占位入口。
    #[serde(skip)]
    child_summary_path: PathBuf,
}

impl SubagentMeta {
    fn title(&self) -> String {
        self.description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| self.subagent_type.clone())
            .unwrap_or_else(|| self.child_session_id.clone())
    }
}

fn parse_optional_timestamp(value: Option<&str>, label: &str) -> Result<Option<i64>> {
    value
        .map(|value| {
            crate::support::time::parse_timestamp(value)
                .with_context(|| format!("Invalid Grok Build {label}"))
        })
        .transpose()
}

#[derive(Clone)]
struct Row {
    summary: SessionSummary,
    meta: Option<SubagentMeta>,
    readable: bool,
}

impl FamilyRow for Row {
    fn member_path(&self) -> Cow<'_, Path> {
        Cow::Borrowed(Path::new(&self.summary.transcript_path))
    }
    fn family_root_id(&self) -> &str {
        &self.summary.source_session_id
    }
    fn member_id(&self) -> &str {
        &self.summary.source_session_id
    }
    fn member_created_at(&self) -> Option<i64> {
        self.summary.created_at
    }
    fn member_updated_at(&self) -> Option<i64> {
        self.summary.updated_at
    }
    fn member_tie_breaker(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.member_id())
    }
}

fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        bail!("Invalid Grok Build id");
    }
    Ok(())
}

fn regular_path(path: &Path) -> Result<()> {
    let configured_root = root()?;
    let root = configured_root.canonicalize()?;
    let relative = path
        .strip_prefix(&root)
        .or_else(|_| path.strip_prefix(&configured_root))
        .context("Grok Build path outside sessions root")?;
    let mut current = root;
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            bail!("Invalid Grok Build path");
        }
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            bail!("Grok Build symlink is not allowed");
        }
    }
    Ok(())
}

fn load_subagent_metas(root: &Path) -> Result<Vec<SubagentMeta>> {
    let mut metas = Vec::new();
    for entry in walkdir::WalkDir::new(root).min_depth(5).max_depth(5) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "meta.json" {
            continue;
        }
        let path = entry.path();
        regular_path(path)?;
        let child_dir = path.parent().context("Invalid subagent directory")?;
        let subagents_dir = child_dir.parent().context("Invalid subagent directory")?;
        if subagents_dir.file_name().and_then(|value| value.to_str()) != Some("subagents") {
            continue;
        }
        let parent_dir = subagents_dir
            .parent()
            .context("Invalid subagent directory")?;
        let bucket = parent_dir.parent().context("Invalid subagent directory")?;
        let mut meta: SubagentMeta = serde_json::from_reader(File::open(path)?)
            .context("Invalid Grok Build subagent meta")?;
        for id in [
            &meta.subagent_id,
            &meta.parent_session_id,
            &meta.child_session_id,
        ] {
            valid_id(id)?;
        }
        let parent_id = parent_dir
            .file_name()
            .and_then(|value| value.to_str())
            .context("Invalid parent directory")?;
        if meta.parent_session_id != parent_id
            || child_dir.file_name().and_then(|value| value.to_str())
                != Some(meta.subagent_id.as_str())
        {
            bail!("Grok Build subagent meta directory mismatch");
        }
        meta.child_summary_path = bucket.join(&meta.child_session_id).join("summary.json");
        metas.push(meta);
    }
    Ok(metas)
}

fn family_index() -> Result<FamilyIndex<Row>> {
    let root = root()?;
    if !root.exists() {
        return Ok(FamilyIndex::build(Vec::new()));
    }
    let root = root.canonicalize()?;
    let mut rows: HashMap<String, Row> = HashMap::new();
    for entry in walkdir::WalkDir::new(&root).min_depth(3).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "summary.json" {
            continue;
        }
        let summary = summary(entry.path())?;
        let readable = match sibling(entry.path(), "chat_history.jsonl")? {
            Some(path) => File::open(path).is_ok(),
            // 尚未写入 history 的会话按 0 条消息读取，而不是当成坏会话。
            None => true,
        };
        let id = summary.source_session_id.clone();
        if rows
            .insert(
                id,
                Row {
                    summary,
                    meta: None,
                    readable,
                },
            )
            .is_some()
        {
            bail!("Conflicting Grok Build session ids");
        }
    }

    // 归属只认 meta：updates 只有事件、工具结果只是文本，都不能作为关系来源。
    let metas = load_subagent_metas(&root)?;
    let mut owners: HashMap<String, String> = HashMap::new();
    let mut subagent_ids = HashSet::new();
    for meta in &metas {
        if !subagent_ids.insert(meta.subagent_id.clone())
            || meta.parent_session_id == meta.child_session_id
            || owners
                .insert(
                    meta.child_session_id.clone(),
                    meta.parent_session_id.clone(),
                )
                .is_some()
        {
            bail!("Conflicting Grok Build subagent ownership");
        }
    }

    // 只折叠父会话存在的一层子代理；父自身也是子代理的嵌套关系尚未验证，
    // 这类子会话保留为独立条目，不推测层级。
    let claimed: HashSet<String> = owners.keys().cloned().collect();
    let mut children: HashMap<String, Vec<Row>> = HashMap::new();
    for meta in &metas {
        if !rows.contains_key(&meta.parent_session_id) || claimed.contains(&meta.parent_session_id)
        {
            continue;
        }
        let mut child = if let Some(row) = rows.get(&meta.child_session_id) {
            let raw: Summary = serde_json::from_reader(File::open(row.member_path())?)?;
            if raw.attempt_id.as_deref() != Some(meta.attempt_id.as_str())
                || raw.session_kind.as_deref() != Some("subagent")
            {
                bail!("Grok Build child summary does not match subagent meta");
            }
            row.clone()
        } else {
            // meta 已声明但子会话缺失：保留可点击入口，打开时再明确报不可读。
            let created_at =
                parse_optional_timestamp(meta.started_at.as_deref(), "subagent started_at")?;
            let updated_at =
                parse_optional_timestamp(meta.completed_at.as_deref(), "subagent completed_at")?
                    .or(created_at);
            Row {
                summary: SessionSummary {
                    source_app: SourceApp::GrokBuild,
                    source_session_id: meta.child_session_id.clone(),
                    title: meta.title(),
                    cwd: None,
                    git_branch: None,
                    transcript_path: meta.child_summary_path.display().to_string(),
                    created_at,
                    updated_at,
                },
                meta: None,
                readable: false,
            }
        };
        child.meta = Some(meta.clone());
        children
            .entry(meta.parent_session_id.clone())
            .or_default()
            .push(child);
    }

    let mut families = Vec::new();
    for (id, row) in &rows {
        if owners
            .get(id)
            .is_some_and(|parent| rows.contains_key(parent) && !claimed.contains(parent))
        {
            continue;
        }
        let mut members = vec![row.clone()];
        members.extend(children.remove(id).unwrap_or_default());
        families.push(Family {
            root: row.clone(),
            members,
        });
    }
    Ok(FamilyIndex::build(families))
}

fn family(path: &Path) -> Result<Family<Row>> {
    let path = validate_path(path)?;
    let index = family_index()?;
    index
        .families
        .iter()
        .find(|family| family.root.member_path().as_ref() == path.as_path())
        .or_else(|| {
            index.families.iter().find(|family| {
                family
                    .members
                    .iter()
                    .any(|row| row.member_path().as_ref() == path.as_path())
            })
        })
        .cloned()
        .context("Grok Build parent session not found")
}

fn family_summary(family: &Family<Row>) -> SessionSummary {
    let mut summary = family.root.summary.clone();
    family.apply_summary_aggregates(&mut summary);
    summary
}

fn marker(row: &Row) -> SessionMessage {
    let meta = row.meta.as_ref().expect("child meta");
    let title = meta.title();
    let mut content = block("output_text", Some(title.clone()));
    content.payload = Some(json!({
        "type": "subagent_started",
        "session_id": row.member_id(),
        "title": title,
        "parent_session_id": meta.parent_session_id,
        "agent_role": meta.subagent_type,
        "status": meta.status,
        "started_at": meta.started_at,
        "completed_at": meta.completed_at,
        "transcript_path": row.summary.transcript_path,
    }));
    SessionMessage {
        id: format!(
            "grok-subagent-{}-{}",
            meta.parent_session_id,
            row.member_id()
        ),
        role: "assistant".to_string(),
        timestamp: row.summary.created_at,
        blocks: vec![content],
        session_id: Some(row.member_id().to_string()),
    }
}

// 主时间线只暴露子代理入口（marker），子会话正文由 parse_agent_messages
// 在打开入口时单独读取；父会话分页因此不必扫描全部子会话。
fn parent_messages(family: &Family<Row>) -> Result<Vec<SessionMessage>> {
    let mut result = messages(family.root.member_path().as_ref())?;
    // history 没有逐条时间，入口单独追加，不推断 spawn 在父会话中的位置。
    result.extend(
        family
            .members
            .iter()
            .filter(|row| row.meta.is_some())
            .map(marker),
    );
    Ok(result)
}

// overview 只列 root 和可读成员的路径；缺失子会话只保留入口，不产生假路径。
fn family_source_paths(family: &Family<Row>) -> Result<Vec<String>> {
    let mut paths = source_paths(family.root.member_path().as_ref())?;
    for row in family
        .members
        .iter()
        .filter(|row| row.meta.is_some() && row.readable)
    {
        paths.extend(source_paths(row.member_path().as_ref())?);
    }
    Ok(paths)
}

// 导出/详情需要成员正文：每个子代理按 marker + 自身消息顺序追加，
// 不伪造跨会话的时间交错。
fn family_messages(family: &Family<Row>) -> Result<Vec<SessionMessage>> {
    let mut result = messages(family.root.member_path().as_ref())?;
    for row in family.members.iter().filter(|row| row.meta.is_some()) {
        if !row.readable {
            bail!("Grok Build child session is not readable");
        }
        result.push(marker(row));
        result.extend(messages(row.member_path().as_ref())?);
    }
    Ok(result)
}

// 所有入口（包括直接传入 transcript_path）都必须留在固定会话布局内。
fn validate_path(path: &Path) -> Result<PathBuf> {
    let root = root()?.canonicalize()?;
    regular_path(path)?;
    let path = path.canonicalize()?;
    let relative = path
        .strip_prefix(&root)
        .context("Grok Build session is outside sessions root")?;
    if relative.components().count() != 3
        || path.file_name().and_then(|v| v.to_str()) != Some("summary.json")
    {
        bail!("Invalid Grok Build summary path");
    }
    Ok(path)
}

fn summary(path: &Path) -> Result<SessionSummary> {
    let path = validate_path(path)?;
    let value: Summary =
        serde_json::from_reader(File::open(&path)?).context("Invalid Grok Build summary")?;
    valid_id(&value.info.id)?;
    if value.chat_format_version != 1 {
        bail!(
            "Unsupported Grok Build chat_format_version: {}",
            value.chat_format_version
        );
    }
    if path
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        != Some(value.info.id.as_str())
    {
        bail!("Grok Build summary id does not match session directory");
    }
    let title = value
        .generated_title
        .filter(|s| !s.trim().is_empty())
        .or_else(|| (!value.session_summary.trim().is_empty()).then_some(value.session_summary))
        .unwrap_or_else(|| value.info.id.clone());
    Ok(SessionSummary {
        source_app: SourceApp::GrokBuild,
        source_session_id: value.info.id,
        title,
        cwd: Some(value.info.cwd),
        git_branch: None,
        transcript_path: path.display().to_string(),
        created_at: Some(
            crate::support::time::parse_timestamp(&value.created_at)
                .context("Invalid Grok Build created_at")?,
        ),
        updated_at: Some(
            crate::support::time::parse_timestamp(&value.updated_at)
                .context("Invalid Grok Build updated_at")?,
        ),
    })
}

fn sibling(path: &Path, name: &str) -> Result<Option<PathBuf>> {
    let path = validate_path(path)?;
    let parent = path.parent().context("Missing session directory")?;
    let candidate = parent.join(name);
    match fs::symlink_metadata(&candidate) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => {
            let resolved = candidate.canonicalize()?;
            if resolved != candidate || !resolved.is_file() {
                bail!("Invalid Grok Build auxiliary file path");
            }
            Ok(Some(resolved))
        }
    }
}

fn scan(path: &Path, name: &str, mut visit: impl FnMut(usize, Value) -> Result<()>) -> Result<()> {
    let Some(file) = sibling(path, name)? else {
        return Ok(());
    };
    for (index, line) in BufReader::new(File::open(file)?).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(&line)
            .with_context(|| format!("Invalid Grok Build {name} JSON at line {}", index + 1))?;
        visit(index, value)?;
    }
    Ok(())
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing Grok Build string field: {key}"))
}

fn block(kind: &str, text: Option<String>) -> ContentBlock {
    ContentBlock {
        kind: kind.into(),
        text,
        tool_name: None,
        tool_call_id: None,
        is_error: None,
        payload: None,
    }
}

fn tool_statuses(path: &Path) -> Result<HashMap<String, bool>> {
    let mut statuses = HashMap::new();
    scan(path, "events.jsonl", |_, value| {
        if value["type"] == "tool_completed" {
            if let (Some(id), Some(outcome)) =
                (value["tool_call_id"].as_str(), value["outcome"].as_str())
            {
                match outcome {
                    "success" => {
                        statuses.insert(id.to_owned(), false);
                    }
                    "error" => {
                        statuses.insert(id.to_owned(), true);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })?;
    scan(path, "updates.jsonl", |_, value| {
        let update = &value["params"]["update"];
        if matches!(
            update["sessionUpdate"].as_str(),
            Some("tool_call" | "tool_call_update")
        ) {
            if let (Some(id), Some(status)) =
                (update["toolCallId"].as_str(), update["status"].as_str())
            {
                match status {
                    "completed" => {
                        statuses.entry(id.to_owned()).or_insert(false);
                    }
                    "failed" => {
                        statuses.insert(id.to_owned(), true);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })?;
    Ok(statuses)
}

fn messages(path: &Path) -> Result<Vec<SessionMessage>> {
    let session_id = summary(path)?.source_session_id;
    let statuses = tool_statuses(path)?;
    let mut messages = Vec::new();
    let mut tools = HashMap::new();
    scan(path, "chat_history.jsonl", |index, value| {
        let kind = required(&value, "type")?;
        let mut blocks = Vec::new();
        let role = match kind {
            "system" => "system",
            "user" => {
                if value["synthetic_reason"] == "system_reminder" {
                    return Ok(());
                }
                for item in value["content"]
                    .as_array()
                    .context("Invalid Grok Build user content")?
                {
                    match required(item, "type")? {
                        "text" => blocks.push(block("text", Some(required(item, "text")?.into()))),
                        "image" => {
                            let mut image = block("image", None);
                            image.payload =
                                Some(json!({"type":"image", "url": required(item, "url")?}));
                            blocks.push(image);
                        }
                        _ => bail!("Unsupported Grok Build user content type"),
                    }
                }
                "user"
            }
            "reasoning" => {
                // 只投影可展示的思考；加密字段不得进入归一化 payload。
                let content = value
                    .get("content")
                    .and_then(Value::as_array)
                    .filter(|items| !items.is_empty())
                    .or_else(|| value.get("summary").and_then(Value::as_array));
                if let Some(items) = content {
                    for item in items {
                        blocks.push(block("thinking", Some(required(item, "text")?.into())));
                    }
                }
                "assistant"
            }
            "assistant" => "assistant",
            "tool_result" => {
                let id = required(&value, "tool_call_id")?;
                let mut result = block("tool_result", Some(required(&value, "content")?.into()));
                result.tool_call_id = Some(id.into());
                result.tool_name = tools.get(id).cloned();
                result.is_error = statuses.get(id).copied();
                blocks.push(result);
                "tool"
            }
            _ => bail!(
                "Unsupported Grok Build chat history type at line {}",
                index + 1
            ),
        };
        if matches!(kind, "system" | "assistant") {
            let text = required(&value, "content")?;
            if !text.is_empty() {
                blocks.push(block("text", Some(text.into())));
            }
        }
        if kind == "assistant" {
            if let Some(calls) = value.get("tool_calls") {
                for call in calls.as_array().context("Invalid Grok Build tool_calls")? {
                    let id = required(call, "id")?;
                    let name = required(call, "name")?;
                    let arguments: Value = serde_json::from_str(required(call, "arguments")?)
                        .context("Invalid Grok Build tool arguments JSON")?;
                    let mut tool = block("tool_use", None);
                    tool.tool_call_id = Some(id.into());
                    tool.tool_name = Some(name.into());
                    tool.payload = Some(json!({"input":arguments}));
                    tools.insert(id.to_string(), name.to_string());
                    blocks.push(tool);
                }
            }
        }
        if !blocks.is_empty() {
            messages.push(SessionMessage {
                id: format!("grok-{session_id}-message-{index}"),
                role: role.into(),
                timestamp: None,
                blocks,
                session_id: Some(session_id.clone()),
            });
        }
        Ok(())
    })?;
    Ok(messages)
}

// 不保留整条 events 时间线，内存开销随页大小而不是日志长度增长。
fn events_page(path: &Path, offset: usize, limit: usize) -> Result<SessionEventPage> {
    let mut events = Vec::new();
    let mut total = 0;
    scan(path, "events.jsonl", |index, value| {
        let kind = required(&value, "type")?;
        if total >= offset && total < offset.saturating_add(limit) {
            let payload = [
                "tool_name",
                "duration_ms",
                "outcome",
                "tool_call_id",
                "turn_number",
                "model_id",
                "session_relationship",
            ]
            .into_iter()
            .filter_map(|key| {
                value
                    .get(key)
                    .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
                    .map(|v| (key.to_owned(), v.clone()))
            })
            .collect::<serde_json::Map<_, _>>();
            events.push(SessionEvent {
                id: format!("grok-event-{index}"),
                kind: kind.into(),
                timestamp: value["ts"]
                    .as_str()
                    .and_then(crate::support::time::parse_timestamp),
                summary: kind.into(),
                payload: (!payload.is_empty()).then_some(Value::Object(payload)),
                session_id: None,
            });
        }
        total += 1;
        Ok(())
    })?;
    let offset = offset.min(total);
    let end = offset.saturating_add(limit).min(total);
    Ok(SessionEventPage {
        events,
        offset,
        limit,
        total_count: total,
        has_more: end < total,
        next_offset: (end < total).then_some(end),
    })
}

fn source_paths(path: &Path) -> Result<Vec<String>> {
    let mut paths = vec![validate_path(path)?.display().to_string()];
    for name in ["chat_history.jsonl", "events.jsonl", "updates.jsonl"] {
        if let Some(path) = sibling(path, name)? {
            paths.push(path.display().to_string());
        }
    }
    Ok(paths)
}

impl SessionReader for GrokBuildBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let mut entries = family_index()?
            .families
            .into_iter()
            .map(|family| {
                let summary = family_summary(&family);
                SessionFileEntry {
                    path: PathBuf::from(&summary.transcript_path),
                    sort_timestamp: summary.updated_at.unwrap_or_default(),
                    summary: Some(summary),
                }
            })
            .collect::<Vec<_>>();
        super::sort_entries(&mut entries);
        Ok(entries)
    }
    fn resolve_path(&self, id: &str) -> Result<PathBuf> {
        valid_id(id)?;
        family_index()?
            .families
            .into_iter()
            .flat_map(|family| family.members)
            .find(|row| row.member_id() == id)
            .map(|row| PathBuf::from(row.summary.transcript_path))
            .context("Grok Build session not found")
    }
    fn parse_summary(&self, path: &Path) -> Result<SessionSummary> {
        // 与 family 聚合语义一致：成员路径解析到所属 family 的 root 摘要。
        let family = family(path)?;
        Ok(family_summary(&family))
    }
    fn parse_overview(&self, path: &Path) -> Result<SessionOverview> {
        let family = family(path)?;
        Ok(SessionOverview {
            summary: family_summary(&family),
            source_paths: family_source_paths(&family)?,
            message_count: None,
            event_count: None,
            agents: family_agents(&family, |row| match &row.meta {
                Some(meta) => FamilyAgentLabel::Child(meta.title()),
                None => FamilyAgentLabel::Root,
            }),
        })
    }
    fn parse_messages_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        let family = family(path)?;
        let (messages, offset, next_offset, total_count) =
            crate::support::paging::slice_page(&parent_messages(&family)?, offset, limit);
        Ok(SessionMessagePage {
            messages,
            offset,
            limit,
            next_offset,
            total_count,
            has_more: next_offset.is_some(),
        })
    }
    fn parse_events_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage> {
        summary(path)?;
        events_page(path, offset, limit)
    }
    fn parse_detail(&self, path: &Path) -> Result<SessionDetail> {
        let family = family(path)?;
        Ok(SessionDetail {
            summary: family_summary(&family),
            source_paths: family_source_paths(&family)?,
            messages: family_messages(&family)?,
            events: events_page(family.root.member_path().as_ref(), 0, usize::MAX)?.events,
        })
    }
    fn parse_agent_messages(
        &self,
        path: &Path,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        let family = family(path)?;
        let row = family
            .members
            .iter()
            .find(|row| row.meta.is_some() && row.member_id() == agent_session_id)
            .context("Grok Build subagent does not belong to this session")?;
        // 子会话缺失或损坏时整个父会话仍可读，只有打开该入口才报错。
        if !row.readable {
            bail!("Grok Build subagent session is not readable");
        }
        // marker + 子会话正文：弹窗依赖 marker 分组，缺失时会退化为空内容。
        let mut result = vec![marker(row)];
        result.extend(messages(row.member_path().as_ref())?);
        Ok(result)
    }
}
