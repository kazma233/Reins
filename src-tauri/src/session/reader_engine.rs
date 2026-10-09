//! 读取器引擎:family 形态会话读取器共享的机械底座。
//!
//! 分组与 root 选取按 family_index 的既定边界留在读取器;本模块拥有其余
//! 全部机械部分——目录索引与双半时间线的 load-through 缓存、三级摘要缓存、
//! marker 外壳与 (timestamp, id) 终排、分页五元组、clear 编排。读取器实现
//! `FamilySpec` 声明真差异,典型来源大部分方法吃默认值。锁与错误文案是
//! 前端可见契约,一律由 label 数据逐字还原。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use super::family_index::{Family, FamilyIndex, FamilyRow};
use super::family_timeline::{FamilyAgentLabel, agent_messages, family_agents};
use super::summary_cache::SummaryDb;
use super::{
    ContentBlock, SessionEvent, SessionEventPage, SessionFileEntry, SessionMessage,
    SessionMessagePage, SessionOverview, SessionReader, SessionSummary, SessionTokenUsage,
    SourceApp,
};

/// 单成员双半时间线:读取器一次扫描产出,engine 负责缓存、marker 注入与终排。
#[derive(Debug)]
pub(crate) struct MemberTimeline {
    pub(crate) messages: Arc<Vec<SessionMessage>>,
    pub(crate) events: Arc<Vec<SessionEvent>>,
}

/// 目录索引的失效口径。engine 只做相等比较、不假设单调性:指纹串把文件
/// 集合变化也计入失效,与 mtime 是两种语义。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Freshness {
    Stamp(i64),
    Fingerprint(String),
}

/// overview 计数三派。
#[derive(Clone, Copy)]
pub(crate) enum OverviewCounts {
    /// 加载双半时间线取 len(有填缓存副作用,时序由 engine 保持)。
    Timeline,
    /// 不计数。
    Omitted,
    /// 读取器给原始计数,engine 补 marker 数。
    Declared,
}

/// family 摘要的合成方式。
#[derive(Clone, Copy)]
pub(crate) enum SummaryKind {
    /// root 转录整文件解析,经三级缓存(内存 → 持久层 → 解析)。
    RootTranscript,
    /// 直接从 root 行合成,无路径级缓存。
    Rows,
}

/// 行装载的坏文件策略。
#[derive(Clone, Copy)]
pub(crate) enum RowErrorPolicy {
    /// 静默跳过。
    SkipSilently,
    /// 记日志跳过(文案 "{label} session skipped {path}: {error}")。
    SkipLogged,
    /// 中止整个列表。
    Abort,
}

/// marker 的读取器供给面:engine 组装外壳(正文/摘要/kind/session_id 与
/// type/title 基底),读取器给 id 方案与 payload 附加字段;dsh 的
/// message/event payload 不对称由两个 extras 分开表达。
pub(crate) struct MarkerShape {
    pub(crate) message_id: String,
    pub(crate) event_id: String,
    pub(crate) message_extras: Value,
    pub(crate) event_extras: Value,
}

impl MarkerShape {
    /// 默认 id 方案 "{label小写}-subagent-start-{id}" / "-subagent-event-{id}"。
    pub(crate) fn labeled(label: &str, member_id: &str) -> Self {
        let prefix = label.to_lowercase();
        Self {
            message_id: format!("{prefix}-subagent-start-{member_id}"),
            event_id: format!("{prefix}-subagent-event-{member_id}"),
            message_extras: json!({}),
            event_extras: json!({}),
        }
    }

    pub(crate) fn extras(mut self, message: Value, event: Value) -> Self {
        self.message_extras = message;
        self.event_extras = event;
        self
    }
}

/// 读取器真差异的声明面。必填项覆盖身份、行发现、分组、双半 loader、
/// agent 名册;其余带默认值,每个覆写点对应一个真实存在的来源差异。
pub(crate) trait FamilySpec: Clone + Send + Sync + 'static {
    type Row: FamilyRow + Clone + Send + Sync;

    fn app(&self) -> SourceApp;
    /// 锁文案与默认 marker id 前缀(如 "Codex")。
    fn label(&self) -> &'static str;
    /// family 未找到错误的显示名;默认等于 label(),产品名与锁前缀拼写
    /// 不同时覆写(claude/dsh 经 agents 清单派生)。
    fn display_label(&self) -> &'static str {
        self.label()
    }
    /// index 锁名前缀;dsh 覆写为 "DSH index"(无 "family" 字样)。
    fn index_lock_label(&self) -> String {
        format!("{} family index", self.label())
    }

    /// 实例 root 下的行扫描根;目录索引的 source_key。
    fn scan_root(&self, root: &Path) -> PathBuf;
    fn list_rows(&self, scan_root: &Path) -> Result<Vec<Self::Row>>;
    /// 分组与 root 选取(孤儿语义等最危险的差异全部活在这里)。
    fn group_families(&self, rows: Vec<Self::Row>) -> Result<Vec<Family<Self::Row>>>;
    /// 目录失效。默认:扫描根目录 mtime 与其下 jsonl 文件 mtime 的最大值。
    fn index_freshness(&self, scan_root: &Path) -> Result<Freshness> {
        subtree_mtime_max(scan_root).map(Freshness::Stamp)
    }

    fn summary_kind(&self) -> SummaryKind {
        SummaryKind::RootTranscript
    }
    /// SummaryKind::RootTranscript 时的整文件解析(三级缓存的 miss loader)。
    fn parse_full_summary(&self, _path: &Path) -> Result<SessionSummary> {
        bail!("{} 未声明 SummaryKind::RootTranscript", self.label())
    }
    /// SummaryKind::Rows 时的行直构。
    fn summary_from_rows(&self, _root: &Self::Row) -> SessionSummary {
        unreachable!("{} 未声明 SummaryKind::Rows", self.label())
    }
    /// 行级 usage(索引行自带);默认无。
    fn row_usage(&self, _row: &Self::Row) -> Option<SessionTokenUsage> {
        None
    }
    /// family 总 usage;默认成员 row_usage 求和。codex 覆写为逐成员经
    /// path_summary 整文件解析(索引行只读首行拿不到 usage)。
    fn family_usage(
        &self,
        engine: &ReaderEngine<Self>,
        family: &Family<Self::Row>,
    ) -> Result<Option<SessionTokenUsage>> {
        let _ = engine;
        Ok(family.sum_token_usage(|row| self.row_usage(row)))
    }

    /// timeline 失效(毫秒口径)。默认:成员文件 mtime 最大值;SQL 来源
    /// 覆写为 max(db mtime, family updated_at),db 路径从 scan_root 派生。
    fn family_freshness(&self, scan_root: &Path, family: &Family<Self::Row>) -> Result<i64> {
        let _ = scan_root;
        member_mtime_max(family)
    }
    /// 逐成员一次扫描产出双半;返回与 members 等长同序。SQL 来源经
    /// scan_root 打开实例 db。
    fn load_members(&self, scan_root: &Path, members: &[Self::Row]) -> Result<Vec<MemberTimeline>>;
    /// marker 注入条件;默认 member_id ≠ root。注意与 (+N) 计数是两条
    /// 独立口径,不得合并(claude 有"有 marker、不计数"的成员)。
    fn is_subagent_member(&self, family: &Family<Self::Row>, row: &Self::Row) -> bool {
        row.member_id() != family.root.member_id()
    }
    /// agent 名册显示名,与 marker 标题共用。
    fn agent_name(&self, row: &Self::Row) -> String;
    fn marker(&self, member_id: &str, row: &Self::Row) -> MarkerShape {
        let _ = row;
        MarkerShape::labeled(self.label(), member_id)
    }
    /// root 转录的前置事件(codex 的 subagent 生命周期事件)。
    fn root_extra_events(&self, _family: &Family<Self::Row>) -> Result<Vec<SessionEvent>> {
        Ok(Vec::new())
    }

    fn overview_counts(&self) -> OverviewCounts {
        OverviewCounts::Timeline
    }
    /// OverviewCounts::Declared 时的原始计数(不含 marker);SQL 来源经
    /// scan_root 打开实例 db。
    fn count_family_records(
        &self,
        _scan_root: &Path,
        _family: &Family<Self::Row>,
    ) -> Result<(usize, usize)> {
        bail!("{} 未声明 OverviewCounts::Declared", self.label())
    }
    fn agent_label(&self, family: &Family<Self::Row>, row: &Self::Row) -> FamilyAgentLabel;
    /// overview.source_paths;默认成员路径,SQL 来源覆写以 db 文件打头。
    fn family_source_paths(&self, _scan_root: &Path, family: &Family<Self::Row>) -> Vec<String> {
        family.source_paths()
    }
    /// parse_agent_messages 的成员归属校验(dsh 专用)。
    fn check_agent_member(
        &self,
        _family: &Family<Self::Row>,
        _agent_session_id: &str,
    ) -> Result<()> {
        Ok(())
    }
}

// —— 双半时间线缓存 ——

struct DualHalfEntry {
    updated_at: i64,
    messages: Option<Arc<Vec<SessionMessage>>>,
    events: Option<Arc<Vec<SessionEvent>>>,
}

/// 双半 load-through 缓存:命中要求 stamp 相等且两半齐全(pi 的既有语义);
/// loader 在锁外执行,miss 时单 loader 一次产出双半。
pub(crate) struct DualHalfCache {
    label: String,
    entries: Mutex<HashMap<String, DualHalfEntry>>,
}

impl DualHalfCache {
    pub(crate) fn new(label: String) -> Self {
        Self {
            label,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn load(
        &self,
        cache_key: String,
        updated_at: i64,
        load: impl FnOnce() -> Result<(Vec<SessionMessage>, Vec<SessionEvent>)>,
    ) -> Result<MemberTimeline> {
        {
            let entries = self.lock()?;
            if let Some(entry) = entries.get(&cache_key) {
                if entry.updated_at == updated_at {
                    if let (Some(messages), Some(events)) = (&entry.messages, &entry.events) {
                        return Ok(MemberTimeline {
                            messages: messages.clone(),
                            events: events.clone(),
                        });
                    }
                }
            }
        }

        let (messages, events) = load()?;
        let timeline = MemberTimeline {
            messages: Arc::new(messages),
            events: Arc::new(events),
        };
        self.lock()?.insert(
            cache_key,
            DualHalfEntry {
                updated_at,
                messages: Some(timeline.messages.clone()),
                events: Some(timeline.events.clone()),
            },
        );
        Ok(timeline)
    }

    pub(crate) fn clear(&self) -> Result<()> {
        self.lock()?.clear();
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, DualHalfEntry>>> {
        self.entries
            .lock()
            .map_err(|_| anyhow!("{} cache lock was poisoned", self.label))
    }
}

// —— 三级摘要缓存(内存 → 持久层 → 整文件解析) ——

struct SummaryMemEntry {
    updated_at: i64,
    summary: SessionSummary,
}

/// 路径级三级摘要缓存:内存 (canonical path_key, mtime) → summary_cache
/// 持久层 → spec 的整文件解析,逐级回填。持久层句柄随引擎实例注入。
pub(crate) struct SummaryMemCache {
    app: SourceApp,
    label: String,
    entries: Mutex<HashMap<String, SummaryMemEntry>>,
    db: SummaryDb,
}

impl SummaryMemCache {
    pub(crate) fn new(app: SourceApp, label: String, db: SummaryDb) -> Self {
        Self {
            app,
            label,
            entries: Mutex::new(HashMap::new()),
            db,
        }
    }

    pub(crate) fn load(
        &self,
        path: &Path,
        parse: impl FnOnce(&Path) -> Result<SessionSummary>,
    ) -> Result<SessionSummary> {
        let key = crate::support::fs::path_key(path);
        let updated_at = crate::support::time::file_modified_timestamp_millis(path)?;

        if let Some(entry) = self
            .lock()?
            .get(&key)
            .filter(|entry| entry.updated_at == updated_at)
        {
            return Ok(entry.summary.clone());
        }

        // 跨进程的持久缓存:冷启动时未变更的文件跳过整文件解析。
        if let Some(summary) = self.db.load(self.app, path, updated_at) {
            self.lock()?.insert(
                key,
                SummaryMemEntry {
                    updated_at,
                    summary: summary.clone(),
                },
            );
            return Ok(summary);
        }

        let summary = parse(path)?;
        self.lock()?.insert(
            key,
            SummaryMemEntry {
                updated_at,
                summary: summary.clone(),
            },
        );
        self.db.store(self.app, path, updated_at, &summary);

        Ok(summary)
    }

    pub(crate) fn clear(&self) -> Result<()> {
        self.lock()?.clear();
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, SummaryMemEntry>>> {
        self.entries
            .lock()
            .map_err(|_| anyhow!("{} cache lock was poisoned", self.label))
    }
}

// —— 引擎本体 ——

struct CachedIndex<Row: FamilyRow> {
    freshness: Freshness,
    index: FamilyIndex<Row>,
}

/// 按 root 构造的引擎实例:持有该来源在该根下的全部缓存。生产经
/// `FamilyReader`(env 根);测试直接构造,完全脱离进程环境。
pub(crate) struct ReaderEngine<R: FamilySpec> {
    spec: R,
    scan_root: PathBuf,
    index: Mutex<Option<CachedIndex<R::Row>>>,
    timeline: DualHalfCache,
    summaries: Option<SummaryMemCache>,
}

impl<R: FamilySpec> ReaderEngine<R> {
    pub(crate) fn new(spec: R, root: PathBuf, store_dir: PathBuf) -> Self {
        let scan_root = spec.scan_root(&root);
        let label = spec.label();
        let timeline = DualHalfCache::new(format!("{label} timeline"));
        let summaries = match spec.summary_kind() {
            SummaryKind::RootTranscript => Some(SummaryMemCache::new(
                spec.app(),
                format!("{label} summary"),
                SummaryDb::at(store_dir),
            )),
            SummaryKind::Rows => None,
        };

        Self {
            spec,
            scan_root,
            index: Mutex::new(None),
            timeline,
            summaries,
        }
    }

    fn lock_index(&self) -> Result<MutexGuard<'_, Option<CachedIndex<R::Row>>>> {
        self.index
            .lock()
            .map_err(|_| anyhow!("{} cache lock was poisoned", self.spec.index_lock_label()))
    }

    pub(crate) fn family_index(&self) -> Result<FamilyIndex<R::Row>> {
        let freshness = self.spec.index_freshness(&self.scan_root)?;

        if let Some(entry) = self
            .lock_index()?
            .as_ref()
            .filter(|entry| entry.freshness == freshness)
        {
            return Ok(entry.index.clone());
        }

        let rows = self.spec.list_rows(&self.scan_root)?;
        let families = self.spec.group_families(rows)?;
        let index = FamilyIndex::build(families);

        *self.lock_index()? = Some(CachedIndex {
            freshness,
            index: index.clone(),
        });

        Ok(index)
    }

    /// 会话身份的唯一寻址入口:id → family 直查,未命中文案逐字保留各家
    /// not-found 契约(dsh 的无 "for" 变体由本公式原样还原)。
    pub(crate) fn family_for_id(&self, source_session_id: &str) -> Result<Family<R::Row>> {
        self.family_index()?
            .family_for_id(source_session_id)
            .ok_or_else(|| {
                anyhow!(
                    "Could not find {} session {source_session_id}",
                    self.spec.display_label()
                )
            })
    }

    /// 路径级三级摘要;仅 SummaryKind::RootTranscript 的来源可用。
    pub(crate) fn path_summary(&self, path: &Path) -> Result<SessionSummary> {
        let cache = self
            .summaries
            .as_ref()
            .ok_or_else(|| anyhow!("{} 未声明 SummaryKind::RootTranscript", self.spec.label()))?;
        cache.load(path, |path| self.spec.parse_full_summary(path))
    }

    pub(crate) fn family_summary(&self, family: &Family<R::Row>) -> Result<SessionSummary> {
        let mut summary = match self.spec.summary_kind() {
            SummaryKind::RootTranscript => self.path_summary(family.root.member_path().as_ref())?,
            SummaryKind::Rows => self.spec.summary_from_rows(&family.root),
        };

        family.apply_summary_aggregates(&mut summary);
        summary.token_usage = self.spec.family_usage(self, family)?;

        Ok(summary)
    }

    pub(crate) fn family_timeline(&self, family: &Family<R::Row>) -> Result<MemberTimeline> {
        let cache_key = family.root.member_id().to_string();
        let freshness = self.spec.family_freshness(&self.scan_root, family)?;
        self.timeline
            .load(cache_key, freshness, || self.load_family_timeline(family))
    }

    // 装配管线:前置事件 → markers → 各成员双半(session_id 兜底)→ 各半终排。
    // marker 与成员项的入列顺序不影响结果(终排键 (timestamp, id) 唯一)。
    fn load_family_timeline(
        &self,
        family: &Family<R::Row>,
    ) -> Result<(Vec<SessionMessage>, Vec<SessionEvent>)> {
        let mut messages = Vec::new();
        let mut events = self.spec.root_extra_events(family)?;

        let timelines = self.spec.load_members(&self.scan_root, &family.members)?;
        for (row, timeline) in family.members.iter().zip(timelines) {
            if self.spec.is_subagent_member(family, row) {
                messages.push(self.marker_message(row));
                events.push(self.marker_event(row));
            }

            let member_id = row.member_id().to_string();
            for mut message in timeline.messages.iter().cloned() {
                message.session_id.get_or_insert(member_id.clone());
                messages.push(message);
            }
            for mut event in timeline.events.iter().cloned() {
                event.session_id.get_or_insert(member_id.clone());
                events.push(event);
            }
        }

        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });
        events.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok((messages, events))
    }

    fn marker_message(&self, row: &R::Row) -> SessionMessage {
        let member_id = row.member_id().to_string();
        let shape = self.spec.marker(&member_id, row);
        let title = self.spec.agent_name(row);

        let mut payload = json!({
            "type": "subagent_started",
            "session_id": member_id,
            "title": title,
        });
        merge_payload(&mut payload, shape.message_extras);

        SessionMessage {
            id: shape.message_id,
            role: "assistant".to_string(),
            timestamp: row.member_created_at(),
            blocks: vec![ContentBlock {
                kind: "output_text".to_string(),
                text: Some(format!("Sub-agent session: {title}\n{member_id}")),
                tool_name: None,
                tool_call_id: None,
                is_error: None,
                payload: Some(payload),
            }],
            session_id: Some(member_id),
        }
    }

    fn marker_event(&self, row: &R::Row) -> SessionEvent {
        let member_id = row.member_id().to_string();
        let shape = self.spec.marker(&member_id, row);
        let title = self.spec.agent_name(row);

        let mut payload = json!({
            "session_id": member_id,
            "title": title,
        });
        merge_payload(&mut payload, shape.event_extras);

        SessionEvent {
            id: shape.event_id,
            kind: "subagent_started".to_string(),
            timestamp: row.member_created_at(),
            summary: format!("Sub-agent session started: {title}"),
            payload: Some(payload),
            session_id: Some(member_id),
        }
    }

    pub(crate) fn clear(&self) -> Result<()> {
        self.timeline.clear()?;
        if let Some(summaries) = &self.summaries {
            summaries.clear()?;
        }
        *self.lock_index()? = None;
        Ok(())
    }
}

impl<R: FamilySpec> SessionReader for ReaderEngine<R> {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let mut entries = self
            .family_index()?
            .families
            .into_iter()
            .map(|family| -> Result<SessionFileEntry> {
                Ok(SessionFileEntry {
                    path: family.root.member_path().into_owned(),
                    source_session_id: family.root.family_root_id().to_string(),
                    sort_timestamp: family.updated_at().unwrap_or_default(),
                    summary: Some(self.family_summary(&family)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        super::sort_entries(&mut entries);
        Ok(entries)
    }

    fn clear_cache(&self) -> Result<()> {
        self.clear()
    }

    fn parse_summary(&self, source_session_id: &str) -> Result<SessionSummary> {
        self.family_summary(&self.family_for_id(source_session_id)?)
    }

    fn parse_overview(&self, source_session_id: &str) -> Result<SessionOverview> {
        let family = self.family_for_id(source_session_id)?;
        let summary = self.family_summary(&family)?;

        let (message_count, event_count) = match self.spec.overview_counts() {
            OverviewCounts::Omitted => (None, None),
            OverviewCounts::Timeline => {
                let timeline = self.family_timeline(&family)?;
                (Some(timeline.messages.len()), Some(timeline.events.len()))
            }
            OverviewCounts::Declared => {
                let (messages, events) =
                    self.spec.count_family_records(&self.scan_root, &family)?;
                let markers = family.members.len().saturating_sub(1);
                (Some(messages + markers), Some(events + markers))
            }
        };

        Ok(SessionOverview {
            summary,
            source_paths: self.spec.family_source_paths(&self.scan_root, &family),
            message_count,
            event_count,
            agents: family_agents(&family, |row| self.spec.agent_label(&family, row)),
        })
    }

    fn parse_messages_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        let timeline = self.family_timeline(&self.family_for_id(source_session_id)?)?;
        Ok(message_page(&timeline.messages, offset, limit))
    }

    fn parse_events_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage> {
        let timeline = self.family_timeline(&self.family_for_id(source_session_id)?)?;
        Ok(event_page(&timeline.events, offset, limit))
    }

    fn parse_agent_messages(
        &self,
        source_session_id: &str,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        let family = self.family_for_id(source_session_id)?;
        self.spec.check_agent_member(&family, agent_session_id)?;
        let timeline = self.family_timeline(&family)?;
        Ok(agent_messages(
            (*timeline.messages).clone(),
            agent_session_id,
        ))
    }
}

// —— 静态前置(env 根 → 实例)——

/// 每 reader 一个 static 前置:按 root() 解析结果缓存引擎实例,换根即整
/// 实例重建(旧实例随 Arc 释放);对外就是 SessionReader。
pub(crate) struct FamilyReader<R: FamilySpec> {
    spec: R,
    root: fn() -> Result<PathBuf>,
    slot: Mutex<Option<(PathBuf, Arc<ReaderEngine<R>>)>>,
}

impl<R: FamilySpec> FamilyReader<R> {
    pub(crate) const fn new(spec: R, root: fn() -> Result<PathBuf>) -> Self {
        Self {
            spec,
            root,
            slot: Mutex::new(None),
        }
    }

    pub(crate) fn engine(&self) -> Result<Arc<ReaderEngine<R>>> {
        let root = (self.root)()?;
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| anyhow!("{} reader engine slot lock was poisoned", self.spec.label()))?;

        if let Some((cached_root, engine)) = slot.as_ref() {
            if *cached_root == root {
                return Ok(engine.clone());
            }
        }

        // 持久缓存目录随实例解析:测试切根即换库,与存量全局层行为一致。
        let store_dir = crate::support::fs::user_home_dir()
            .map(|home| home.join(".reins"))
            .ok_or_else(|| anyhow!("Unable to determine home directory"))?;

        let engine = Arc::new(ReaderEngine::new(
            self.spec.clone(),
            root.clone(),
            store_dir,
        ));
        *slot = Some((root, engine.clone()));
        Ok(engine)
    }
}

impl<R: FamilySpec> SessionReader for FamilyReader<R> {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        self.engine()?.list_entries()
    }

    fn clear_cache(&self) -> Result<()> {
        self.engine()?.clear_cache()
    }

    fn parse_summary(&self, source_session_id: &str) -> Result<SessionSummary> {
        self.engine()?.parse_summary(source_session_id)
    }

    fn parse_overview(&self, source_session_id: &str) -> Result<SessionOverview> {
        self.engine()?.parse_overview(source_session_id)
    }

    fn parse_messages_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        self.engine()?
            .parse_messages_page(source_session_id, offset, limit)
    }

    fn parse_events_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage> {
        self.engine()?
            .parse_events_page(source_session_id, offset, limit)
    }

    fn parse_agent_messages(
        &self,
        source_session_id: &str,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        self.engine()?
            .parse_agent_messages(source_session_id, agent_session_id)
    }
}

// —— 共享小件 ——

/// 分页五元组装配:所有 family 来源共用的页面形状。
pub(crate) fn message_page(
    messages: &[SessionMessage],
    offset: usize,
    limit: usize,
) -> SessionMessagePage {
    let (messages, start, next_offset, total_count) =
        crate::support::paging::slice_page(messages, offset, limit);
    SessionMessagePage {
        messages,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    }
}

pub(crate) fn event_page(events: &[SessionEvent], offset: usize, limit: usize) -> SessionEventPage {
    let (events, start, next_offset, total_count) =
        crate::support::paging::slice_page(events, offset, limit);
    SessionEventPage {
        events,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    }
}

/// 枚举后的行装载:按来源声明的坏文件策略处理解析失败。
pub(crate) fn scan_files<Row>(
    files: Vec<PathBuf>,
    parse: impl Fn(&Path) -> Result<Row>,
    policy: RowErrorPolicy,
    label: &str,
) -> Result<Vec<Row>> {
    let mut rows = Vec::with_capacity(files.len());
    for path in files {
        match parse(&path) {
            Ok(row) => rows.push(row),
            Err(error) => match policy {
                RowErrorPolicy::Abort => return Err(error),
                RowErrorPolicy::SkipSilently => {}
                RowErrorPolicy::SkipLogged => {
                    // 单个损坏文件跳过,不让一个坏文件隐藏其余会话。
                    crate::logger::log_error(format!(
                        "{label} session skipped {}: {error}",
                        path.display()
                    ));
                }
            },
        }
    }
    Ok(rows)
}

fn subtree_mtime_max(root: &Path) -> Result<i64> {
    // 扫描根可能还没落地(codex 的 sessions 子目录随首次会话才创建),缺失按空
    // 目录处理,让索引走到空列表分支而不是在 metadata 读取上失败。
    if !root.is_dir() {
        return Ok(0);
    }

    let mut latest = crate::support::time::file_modified_timestamp_millis(root)?;

    for path in crate::support::fs::enumerate_jsonl_files(root)? {
        latest = latest.max(crate::support::time::file_modified_timestamp_millis(&path)?);
    }

    Ok(latest)
}

fn member_mtime_max<Row: FamilyRow>(family: &Family<Row>) -> Result<i64> {
    family.members.iter().try_fold(0, |latest, row| {
        Ok(
            latest.max(crate::support::time::file_modified_timestamp_millis(
                row.member_path().as_ref(),
            )?),
        )
    })
}

fn merge_payload(base: &mut Value, extras: Value) {
    if let (Some(base), Value::Object(extras)) = (base.as_object_mut(), extras) {
        for (key, value) in extras {
            base.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use anyhow::{Result, bail};
    use serde_json::json;

    use super::*;

    // —— 夹具:一个最小 family 来源,行为经 Config 切换各派系 ——

    #[derive(Clone, Debug)]
    struct TestRow {
        id: String,
        path: PathBuf,
    }

    impl FamilyRow for TestRow {
        fn member_path(&self) -> Cow<'_, Path> {
            Cow::Borrowed(&self.path)
        }

        fn family_root_id(&self) -> &str {
            &self.id
        }

        fn member_id(&self) -> &str {
            &self.id
        }

        fn member_created_at(&self) -> Option<i64> {
            Some(1)
        }

        fn member_updated_at(&self) -> Option<i64> {
            Some(2)
        }

        fn member_tie_breaker(&self) -> Cow<'_, str> {
            Cow::Borrowed(&self.id)
        }
    }

    #[derive(Clone)]
    struct TestSpec {
        freshness: Freshness,
        counts: OverviewCounts,
        summary: SummaryKind,
        row_policy: RowErrorPolicy,
        scan_calls: Arc<AtomicUsize>,
        parse_calls: Arc<AtomicUsize>,
    }

    impl TestSpec {
        fn default_spec() -> Self {
            Self {
                freshness: Freshness::Stamp(0),
                counts: OverviewCounts::Timeline,
                summary: SummaryKind::RootTranscript,
                row_policy: RowErrorPolicy::Abort,
                scan_calls: Arc::new(AtomicUsize::new(0)),
                parse_calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl FamilySpec for TestSpec {
        type Row = TestRow;

        fn app(&self) -> SourceApp {
            SourceApp::Codex
        }

        fn label(&self) -> &'static str {
            "Test"
        }

        fn index_lock_label(&self) -> String {
            "Test family index".to_string()
        }

        fn scan_root(&self, root: &Path) -> PathBuf {
            root.join("scan")
        }

        fn list_rows(&self, scan_root: &Path) -> Result<Vec<TestRow>> {
            self.scan_calls.fetch_add(1, Ordering::SeqCst);

            let files = crate::support::fs::enumerate_jsonl_files(scan_root)?;
            let rows = scan_files(
                files,
                |path| {
                    if path.file_stem().is_some_and(|stem| stem == "broken") {
                        bail!("broken row");
                    }
                    Ok(TestRow {
                        id: path
                            .file_stem()
                            .expect("file stem")
                            .to_string_lossy()
                            .into_owned(),
                        path: path.to_path_buf(),
                    })
                },
                self.row_policy,
                "Test",
            )?;

            Ok(rows.into_iter().filter(|row| row.id != "ignore").collect())
        }

        fn group_families(&self, rows: Vec<TestRow>) -> Result<Vec<Family<TestRow>>> {
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            let root = rows
                .iter()
                .find(|row| row.id == "root")
                .cloned()
                .expect("root row");
            Ok(vec![Family {
                root: root.clone(),
                members: {
                    let mut members = vec![root];
                    members.extend(rows.into_iter().filter(|row| row.id != "root"));
                    members
                },
            }])
        }

        fn index_freshness(&self, scan_root: &Path) -> Result<Freshness> {
            match &self.freshness {
                Freshness::Stamp(0) => subtree_mtime_max(scan_root).map(Freshness::Stamp),
                other => Ok(other.clone()),
            }
        }

        fn summary_kind(&self) -> SummaryKind {
            self.summary
        }

        fn parse_full_summary(&self, path: &Path) -> Result<SessionSummary> {
            self.parse_calls.fetch_add(1, Ordering::SeqCst);
            Ok(SessionSummary {
                source_app: SourceApp::Codex,
                source_session_id: "root".to_string(),
                title: format!("parsed {}", path.display()),
                cwd: None,
                git_branch: None,
                transcript_path: path.display().to_string(),
                created_at: Some(1),
                updated_at: Some(2),
                token_usage: None,
            })
        }

        fn summary_from_rows(&self, root: &TestRow) -> SessionSummary {
            SessionSummary {
                source_app: SourceApp::Codex,
                source_session_id: root.id.clone(),
                title: format!("rows {}", root.id),
                cwd: None,
                git_branch: None,
                transcript_path: root.path.display().to_string(),
                created_at: Some(1),
                updated_at: Some(2),
                token_usage: None,
            }
        }

        fn load_members(
            &self,
            _scan_root: &Path,
            members: &[TestRow],
        ) -> Result<Vec<MemberTimeline>> {
            Ok(members
                .iter()
                .map(|row| MemberTimeline {
                    messages: Arc::new(vec![SessionMessage {
                        id: format!("m-{}", row.id),
                        role: "user".to_string(),
                        timestamp: Some(10),
                        blocks: Vec::new(),
                        session_id: None,
                    }]),
                    events: Arc::new(vec![SessionEvent {
                        id: format!("e-{}", row.id),
                        kind: "generic".to_string(),
                        timestamp: Some(10),
                        summary: String::new(),
                        payload: None,
                        session_id: None,
                    }]),
                })
                .collect())
        }

        fn agent_name(&self, row: &TestRow) -> String {
            format!("agent {}", row.id)
        }

        fn marker(&self, member_id: &str, _row: &TestRow) -> MarkerShape {
            MarkerShape::labeled("Test", member_id)
                .extras(json!({"extra": "message"}), json!({"extra": "event"}))
        }

        fn overview_counts(&self) -> OverviewCounts {
            self.counts
        }

        fn count_family_records(
            &self,
            _scan_root: &Path,
            _family: &Family<TestRow>,
        ) -> Result<(usize, usize)> {
            Ok((1, 1))
        }

        fn agent_label(&self, family: &Family<TestRow>, row: &TestRow) -> FamilyAgentLabel {
            if row.id == family.root.id {
                FamilyAgentLabel::Root
            } else {
                FamilyAgentLabel::Child(format!("agent {}", row.id))
            }
        }
    }

    fn fixture(paths: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("reins-engine-{}", uuid::Uuid::new_v4()));
        for path in paths {
            let file = dir.join("scan").join(path);
            fs::create_dir_all(file.parent().expect("parent")).unwrap();
            fs::write(&file, "line\n").unwrap();
        }
        dir
    }

    fn store_dir() -> PathBuf {
        std::env::temp_dir().join(format!("reins-engine-store-{}", uuid::Uuid::new_v4()))
    }

    fn engine(spec: TestSpec, root: PathBuf) -> ReaderEngine<TestSpec> {
        ReaderEngine::new(spec, root, store_dir())
    }

    #[test]
    fn index_cache_hit_skips_rescan() -> Result<()> {
        let dir = fixture(&["root.jsonl", "child.jsonl"]);
        let spec = TestSpec::default_spec();
        let reader = engine(spec.clone(), dir.clone());

        let first = reader.list_entries()?;
        let second = reader.list_entries()?;

        assert_eq!(first.len(), second.len());
        assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn fingerprint_change_rebuilds_index() -> Result<()> {
        let dir = fixture(&["root.jsonl"]);
        let spec = TestSpec {
            freshness: Freshness::Fingerprint("v1".to_string()),
            ..TestSpec::default_spec()
        };
        let reader = engine(spec.clone(), dir.clone());
        reader.list_entries()?;
        assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 1);

        // 指纹口径与 mtime 无关,同指纹命中、变指纹重建。
        let reader = {
            let mut reader = reader;
            reader.spec.freshness = Freshness::Fingerprint("v2".to_string());
            reader
        };
        reader.list_entries()?;
        assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 2);

        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    // id 未命中的文案契约:formula 无 "for"(dsh 现状文案由该公式原样还原),
    // display_label 覆写(claude 的带空格拼写)直达错误消息。
    #[test]
    fn unknown_id_reports_display_label() {
        let dir = fixture(&["root.jsonl"]);
        let reader = engine(TestSpec::default_spec(), dir.clone());

        let error = reader.parse_summary("missing").expect_err("not found");

        assert_eq!(error.to_string(), "Could not find Test session missing");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn timeline_injects_markers_backfills_and_sorts() -> Result<()> {
        let dir = fixture(&["root.jsonl", "child.jsonl"]);
        let reader = engine(TestSpec::default_spec(), dir.clone());

        let page = reader.parse_messages_page("root", 0, 10)?;
        let ids: Vec<&str> = page.messages.iter().map(|m| m.id.as_str()).collect();
        // marker(created_at=1) 先于成员消息(timestamp=10),id 唯一定序。
        assert_eq!(ids, ["test-subagent-start-child", "m-child", "m-root"]);
        // session_id 兜底为成员 id。
        assert_eq!(page.messages[1].session_id.as_deref(), Some("child"));

        let marker = &page.messages[0];
        let payload = marker.blocks[0].payload.as_ref().expect("payload");
        assert_eq!(payload["type"], "subagent_started");
        assert_eq!(payload["extra"], "message");
        assert_eq!(
            marker.blocks[0].text.as_deref(),
            Some("Sub-agent session: agent child\nchild")
        );

        let events = reader.parse_events_page("root", 0, 10)?;
        let event_ids: Vec<&str> = events.events.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            event_ids,
            ["test-subagent-event-child", "e-child", "e-root"]
        );
        let marker_event = &events.events[0];
        assert_eq!(marker_event.kind, "subagent_started");
        assert_eq!(
            marker_event.summary,
            "Sub-agent session started: agent child"
        );
        assert_eq!(
            marker_event.payload.as_ref().expect("payload")["extra"],
            "event"
        );

        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn overview_counts_dispatch_three_ways() -> Result<()> {
        let dir = fixture(&["root.jsonl", "child.jsonl"]);

        let omitted = engine(
            TestSpec {
                counts: OverviewCounts::Omitted,
                ..TestSpec::default_spec()
            },
            dir.clone(),
        );
        let overview = omitted.parse_overview("root")?;
        assert_eq!((overview.message_count, overview.event_count), (None, None));

        let timeline = engine(TestSpec::default_spec(), dir.clone());
        let overview = timeline.parse_overview("root")?;
        // timeline 派:成员双半 + marker(3 条消息、3 个事件)。
        assert_eq!(
            (overview.message_count, overview.event_count),
            (Some(3), Some(3))
        );

        let declared = engine(
            TestSpec {
                counts: OverviewCounts::Declared,
                ..TestSpec::default_spec()
            },
            dir.clone(),
        );
        let overview = declared.parse_overview("root")?;
        // Declared 派:原始计数 1 + marker 数 1。
        assert_eq!(
            (overview.message_count, overview.event_count),
            (Some(2), Some(2))
        );

        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn summary_from_rows_and_root_transcript_paths() -> Result<()> {
        let dir = fixture(&["root.jsonl"]);

        let rows = engine(
            TestSpec {
                summary: SummaryKind::Rows,
                ..TestSpec::default_spec()
            },
            dir.clone(),
        );
        let summary = rows.parse_summary("root")?;
        assert_eq!(summary.title, "rows root");

        let transcript = engine(TestSpec::default_spec(), dir.clone());
        let summary = transcript.parse_summary("root")?;
        assert!(summary.title.starts_with("parsed "));
        assert_eq!(transcript.spec.parse_calls.load(Ordering::SeqCst), 1);

        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn root_transcript_summary_persists_across_instances() -> Result<()> {
        let dir = fixture(&["root.jsonl"]);
        let store = store_dir();
        let spec = TestSpec::default_spec();

        let first = ReaderEngine::new(spec.clone(), dir.clone(), store.clone());
        first.parse_summary("root")?;
        assert_eq!(spec.parse_calls.load(Ordering::SeqCst), 1);

        // 新实例内存缓存为空,应命中持久层而不是重新解析。
        let second = ReaderEngine::new(spec.clone(), dir.clone(), store.clone());
        second.parse_summary("root")?;
        assert_eq!(spec.parse_calls.load(Ordering::SeqCst), 1);

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&store).ok();
        Ok(())
    }

    #[test]
    fn clear_forces_full_rescan() -> Result<()> {
        let dir = fixture(&["root.jsonl"]);
        let spec = TestSpec::default_spec();
        let reader = engine(spec.clone(), dir.clone());

        reader.list_entries()?;
        reader.clear_cache()?;
        reader.list_entries()?;

        assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 2);
        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn scan_files_applies_row_error_policies() -> Result<()> {
        let dir = fixture(&["root.jsonl"]);
        let files = vec![dir.join("root.jsonl"), dir.join("broken.jsonl")];

        let error = scan_files(
            files.clone(),
            |path| {
                if path.file_stem().is_some_and(|stem| stem == "broken") {
                    bail!("broken row")
                }
                Ok(1)
            },
            RowErrorPolicy::Abort,
            "Test",
        )
        .expect_err("abort propagates");
        assert!(error.to_string().contains("broken row"));

        for policy in [RowErrorPolicy::SkipSilently, RowErrorPolicy::SkipLogged] {
            let rows = scan_files(
                files.clone(),
                |path| {
                    if path.file_stem().is_some_and(|stem| stem == "broken") {
                        bail!("broken row")
                    }
                    Ok(1)
                },
                policy,
                "Test",
            )?;
            assert_eq!(rows, vec![1]);
        }

        fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    #[test]
    fn family_reader_rebuilds_engine_when_root_changes() -> Result<()> {
        let home_a = fixture(&["root.jsonl"]);
        let home_b = fixture(&["root.jsonl", "child.jsonl"]);
        let spec = TestSpec::default_spec();

        fn env_root() -> Result<PathBuf> {
            crate::support::fs::user_home_dir().ok_or_else(|| anyhow::anyhow!("no home"))
        }

        let reader = FamilyReader::new(spec.clone(), env_root);
        {
            let _guard = crate::test_support::TestEnvGuard::set_home(&home_a);
            assert_eq!(reader.list_entries()?.len(), 1);
            assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 1);
            // 同根复用实例,不重扫。
            reader.list_entries()?;
            assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 1);
        }
        {
            let _guard = crate::test_support::TestEnvGuard::set_home(&home_b);
            // 换根即整实例重建,重扫新根。
            reader.list_entries()?;
            assert_eq!(spec.scan_calls.load(Ordering::SeqCst), 2);
        }

        fs::remove_dir_all(&home_a).ok();
        fs::remove_dir_all(&home_b).ok();
        Ok(())
    }

    #[test]
    fn poisoned_lock_labels_carry_spec_wording() {
        let cache = DualHalfCache::new("Test timeline".to_string());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.lock().unwrap();
            panic!("poison");
        }));
        let error = cache
            .load("key".into(), 1, || Ok((Vec::new(), Vec::new())))
            .expect_err("poisoned");
        assert_eq!(error.to_string(), "Test timeline cache lock was poisoned");

        let dir = fixture(&["root.jsonl"]);
        let reader = engine(TestSpec::default_spec(), dir.clone());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = reader.lock_index().unwrap();
            panic!("poison");
        }));
        let error = reader.list_entries().expect_err("poisoned");
        assert_eq!(
            error.to_string(),
            "Test family index cache lock was poisoned"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
