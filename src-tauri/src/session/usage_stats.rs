//! token 用量统计:按消耗事件的时间戳把各来源用量归到本地时区的小时,
//! 日序列与今日小时序列都由小时桶聚合而来。
//!
//! 跨天会话由消息/turn/事件级时间戳天然拆分,不做会话级近似。归一后的
//! SessionTokenUsage 直接累加,与列表页 family 求和同口径(resume 段与
//! subagent 线程是独立转录文件,各自归桶后求和)。窗口裁剪与指标推导都在
//! 前端完成。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::model::{
    SessionTokenUsage, SourceApp, UsageDayPoint, UsageHourBuckets, UsageHourPoint,
    UsageSourceStats, UsageStats,
};

/// 来源注册表里一个来源的用量采集方式:三种接入姿势,归桶机器都在本模块,
/// 差异只剩函数引用。
#[derive(Clone, Copy)]
pub(crate) enum UsageKind {
    /// jsonl 目录扫描 + mtime 命中持久缓存,未变更文件跳过整文件解析
    /// (codex/claude/pi);扫描目录是来源根目录或其子目录。
    CachedJsonl {
        subdir: Option<&'static str>,
        extract: fn(&Path) -> Result<UsageHourBuckets>,
    },
    /// 来源自带的转录文件列表,逐文件直接解析不走持久缓存
    /// (grokbuild 的 usage.json、dsh 内嵌 usage 的多帧 zstd);根目录复用
    /// 注册表的 root。
    RawFiles {
        paths: fn() -> Result<Vec<PathBuf>>,
        extract: fn(&Path) -> Result<UsageHourBuckets>,
    },
    /// 从 sqlite 直查(opencode/zcode)。
    Sql {
        collect: fn() -> Result<Option<SqlUsageHours>>,
    },
}

pub(crate) fn usage_stats_inner() -> Result<UsageStats> {
    let outcomes = super::sources::parallel_collect("Usage stats", |spec| spec.usage_days());

    let mut sources = Vec::new();
    for outcome in outcomes {
        if let Some(stats) = outcome? {
            sources.push(stats);
        }
    }

    Ok(UsageStats { sources })
}

// 单文件失败只记日志跳过,不阻断统计。
pub(crate) fn days_from_cached_jsonl(
    source_app: SourceApp,
    root: PathBuf,
    extract: fn(&Path) -> Result<UsageHourBuckets>,
) -> Result<Option<UsageSourceStats>> {
    if !root.exists() {
        return Ok(None);
    }

    let today = today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut session_count = 0usize;
    let mut today_session_count = 0usize;

    for path in crate::support::fs::enumerate_jsonl_files(&root)? {
        let mtime = match crate::support::time::file_modified_timestamp_millis(&path) {
            Ok(mtime) => mtime,
            Err(error) => {
                crate::logger::log_error(format!(
                    "usage stats skipped {}: {error}",
                    path.display()
                ));
                continue;
            }
        };

        let file_hours = match super::usage_day_cache::load(source_app, &path, mtime) {
            Some(cached) => cached,
            None => match extract(&path) {
                Ok(parsed) => {
                    super::usage_day_cache::store(source_app, &path, mtime, &parsed);
                    parsed
                }
                Err(error) => {
                    crate::logger::log_error(format!(
                        "usage stats skipped {}: {error}",
                        path.display()
                    ));
                    continue;
                }
            },
        };

        if file_hours.is_empty() {
            continue;
        }

        session_count += 1;
        if file_has_today(&file_hours, &today) {
            today_session_count += 1;
        }
        merge_buckets(&mut buckets, &file_hours);
    }

    Ok(Some(source_stats(
        source_app,
        buckets,
        session_count,
        today_session_count,
    )))
}

// 根目录不存在表示来源不可用;文件列表由来源自带,与列表扫描同一发现规则,
// 单文件失败只记日志跳过。
pub(crate) fn days_from_paths(
    source_app: SourceApp,
    root: &Path,
    paths: fn() -> Result<Vec<PathBuf>>,
    extract: fn(&Path) -> Result<UsageHourBuckets>,
) -> Result<Option<UsageSourceStats>> {
    if !root.exists() {
        return Ok(None);
    }

    let today = today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut session_count = 0usize;
    let mut today_session_count = 0usize;

    for path in paths()? {
        let file_hours = match extract(&path) {
            Ok(hours) => hours,
            Err(error) => {
                crate::logger::log_error(format!(
                    "usage stats skipped {}: {error}",
                    path.display()
                ));
                continue;
            }
        };

        if file_hours.is_empty() {
            continue;
        }

        session_count += 1;
        if file_has_today(&file_hours, &today) {
            today_session_count += 1;
        }
        merge_buckets(&mut buckets, &file_hours);
    }

    Ok(Some(source_stats(
        source_app,
        buckets,
        session_count,
        today_session_count,
    )))
}

// SQL 来源(opencode/zcode)的小时桶与两级会话数;db 缺失表示来源不可用。
pub(crate) struct SqlUsageHours {
    pub(crate) buckets: UsageHourBuckets,
    pub(crate) session_count: usize,
    pub(crate) today_session_count: usize,
}

// SQL 来源行级用量的原始列:各来源 SQL 只产原始列,口径统一在这里,
// 改口径(如将来加 token 桶)只改这一处。输入列的语义因来源而异:zcode 的
// input_tokens 是全口径(未命中 + 缓存读),opencode 的 tokens.input 已
// 不含缓存读,因此入口分两个函数显式声明。
pub(crate) struct UsageColumns {
    pub(crate) input: i64,
    pub(crate) output: i64,
    pub(crate) reasoning: i64,
    pub(crate) cache_read: i64,
    pub(crate) cache_write: i64,
}

// opencode 形态:输入列已不含缓存读,只并 output + reasoning,负值钳 0。
pub(crate) fn split_reasoning_usage(columns: UsageColumns) -> SessionTokenUsage {
    SessionTokenUsage {
        input_tokens: columns.input.max(0) as u64,
        output_tokens: (columns.output + columns.reasoning).max(0) as u64,
        cache_read_tokens: columns.cache_read.max(0) as u64,
        cache_write_tokens: columns.cache_write.max(0) as u64,
    }
}

// zcode 形态:输入列是全口径、含缓存读,先拆掉缓存读再走同一口径,对齐
// SessionTokenUsage 的 input 不含命中。
pub(crate) fn full_scope_usage(columns: UsageColumns) -> SessionTokenUsage {
    split_reasoning_usage(UsageColumns {
        input: columns.input - columns.cache_read,
        ..columns
    })
}

// SQL 来源行级用量归小时桶的公共尾部:两级会话数与桶合并。行由来源 SQL
// 装配为 (session_id, 毫秒时间戳, 归一后用量)。
pub(crate) fn hours_from_usage_rows(
    rows: impl Iterator<Item = rusqlite::Result<(String, i64, SessionTokenUsage)>>,
) -> Result<SqlUsageHours> {
    let today = today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut sessions = std::collections::HashSet::new();
    let mut today_sessions = std::collections::HashSet::new();

    for row in rows {
        let (session_id, timestamp, usage) = row?;
        let hour_key = hour_key(timestamp);
        if let Some(key) = &hour_key {
            if today_hour(key, &today).is_some() {
                today_sessions.insert(session_id.clone());
            }
        }
        sessions.insert(session_id);
        merge_usage_bucket(&mut buckets, hour_key, usage);
    }

    Ok(SqlUsageHours {
        buckets,
        session_count: sessions.len(),
        today_session_count: today_sessions.len(),
    })
}

pub(crate) fn days_from_sql(
    source_app: SourceApp,
    collect: fn() -> Result<Option<SqlUsageHours>>,
) -> Result<Option<UsageSourceStats>> {
    let Some(hours) = collect()? else {
        return Ok(None);
    };

    Ok(Some(source_stats(
        source_app,
        hours.buckets,
        hours.session_count,
        hours.today_session_count,
    )))
}

fn source_stats(
    source_app: SourceApp,
    buckets: UsageHourBuckets,
    session_count: usize,
    today_session_count: usize,
) -> UsageSourceStats {
    let today = today_prefix();
    UsageSourceStats {
        source_app,
        days: day_points(&buckets),
        today_hours: today_hour_points(&buckets, &today),
        session_count,
        today_session_count,
    }
}

// 小时桶按天前缀合并成日序列。
fn day_points(buckets: &UsageHourBuckets) -> Vec<UsageDayPoint> {
    let mut days: BTreeMap<&str, SessionTokenUsage> = BTreeMap::new();
    for (hour_key, usage) in buckets {
        let day = hour_key
            .split_once('T')
            .map(|(day, _)| day)
            .unwrap_or(hour_key);
        days.entry(day).or_default().accumulate(usage);
    }

    days.into_iter()
        .map(|(day, usage)| UsageDayPoint {
            day: day.to_string(),
            usage,
        })
        .collect()
}

fn today_hour_points(buckets: &UsageHourBuckets, today: &str) -> Vec<UsageHourPoint> {
    buckets
        .iter()
        .filter_map(|(hour_key, usage)| {
            today_hour(hour_key, today).map(|hour| UsageHourPoint {
                hour,
                usage: *usage,
            })
        })
        .collect()
}

// "YYYY-MM-DDTHH" 命中今天时取出小时数;跨午夜后旧前缀自然失配。
pub(crate) fn today_hour(hour_key: &str, today: &str) -> Option<u8> {
    let hour = hour_key.strip_prefix(today)?.strip_prefix('T')?;
    hour.parse().ok()
}

fn file_has_today(buckets: &UsageHourBuckets, today: &str) -> bool {
    buckets.keys().any(|key| today_hour(key, today).is_some())
}

fn merge_buckets(total: &mut UsageHourBuckets, next: &UsageHourBuckets) {
    for (hour_key, usage) in next {
        total.entry(hour_key.clone()).or_default().accumulate(usage);
    }
}

// 消耗事件归桶入口:时间戳缺失的记录不进曲线(总量仍由列表 summary 保证)。
pub(crate) fn merge_usage_bucket(
    buckets: &mut UsageHourBuckets,
    hour_key: Option<String>,
    usage: SessionTokenUsage,
) {
    let Some(hour_key) = hour_key else {
        return;
    };
    buckets.entry(hour_key).or_default().accumulate(&usage);
}

// 天/小时边界取本地时区,与用户在 CLI 工具里看到的活跃日期一致。
pub(crate) fn hour_key(timestamp_ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(timestamp_ms).map(|value| {
        value
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H")
            .to_string()
    })
}

pub(crate) fn today_prefix() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
