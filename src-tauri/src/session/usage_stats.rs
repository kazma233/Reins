//! token 用量统计:按消耗事件的时间戳把各来源用量归到本地时区的小时,
//! 日序列与今日小时序列都由小时桶聚合而来。
//!
//! 跨天会话由消息/turn/事件级时间戳天然拆分,不做会话级近似。归一后的
//! SessionTokenUsage 直接累加,与列表页 family 求和同口径(resume 段与
//! subagent 线程是独立转录文件,各自归桶后求和)。窗口裁剪与指标推导都在
//! 前端完成。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;

use anyhow::{Result, anyhow};

use super::model::{
    SessionTokenUsage, SourceApp, UsageDayPoint, UsageHourBuckets, UsageHourPoint,
    UsageSourceStats, UsageStats,
};
use super::{claude_code, codex, dsh, grokbuild, opencode, pi, zcode};

pub(crate) fn usage_stats_inner() -> Result<UsageStats> {
    let outcomes = thread::scope(|scope| {
        let codex_handle = scope.spawn(|| {
            source_days_from_files(
                SourceApp::Codex,
                codex::root()?.join("sessions"),
                codex::usage_hours,
            )
        });
        let claude_handle = scope.spawn(|| {
            source_days_from_files(
                SourceApp::ClaudeCode,
                claude_code::root()?.join("projects"),
                claude_code::usage_hours,
            )
        });
        let pi_handle = scope
            .spawn(|| source_days_from_files(SourceApp::Pi, pi::sessions_root()?, pi::usage_hours));
        let grokbuild_handle = scope.spawn(grokbuild_days);
        let opencode_handle =
            scope.spawn(|| sql_source_days(SourceApp::OpenCode, opencode::usage_hours));
        let zcode_handle = scope.spawn(|| sql_source_days(SourceApp::Zcode, zcode::usage_hours));
        let dsh_handle = scope.spawn(dsh_days);

        vec![
            codex_handle.join(),
            claude_handle.join(),
            pi_handle.join(),
            grokbuild_handle.join(),
            opencode_handle.join(),
            zcode_handle.join(),
            dsh_handle.join(),
        ]
    });

    let mut sources = Vec::new();
    for outcome in outcomes {
        let stats = outcome.map_err(|_| anyhow!("Usage stats thread panicked"))??;
        if let Some(stats) = stats {
            sources.push(stats);
        }
    }

    Ok(UsageStats { sources })
}

// jsonl 来源(claude/codex/pi):按 mtime 命中小时桶持久缓存,未变更文件跳过
// 整文件解析;单文件失败只记日志跳过,不阻断统计。
fn source_days_from_files(
    source_app: SourceApp,
    root: PathBuf,
    extract: fn(&std::path::Path) -> Result<UsageHourBuckets>,
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

// GrokBuild 的 usage.json 是几 KB 的小文件,直接读,不走持久缓存。
fn grokbuild_days() -> Result<Option<UsageSourceStats>> {
    if !grokbuild::root()?.exists() {
        return Ok(None);
    }

    let today = today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut session_count = 0usize;
    let mut today_session_count = 0usize;

    for path in grokbuild::session_summary_paths()? {
        let file_hours = match grokbuild::usage_hours(&path) {
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
        SourceApp::GrokBuild,
        buckets,
        session_count,
        today_session_count,
    )))
}

// dsh 的 usage 内嵌在转录里,读取要先解压多帧 zstd,直接逐文件解析,不走
// 持久缓存;单文件失败只记日志跳过,与列表扫描同一发现规则。
fn dsh_days() -> Result<Option<UsageSourceStats>> {
    if !dsh::sessions_root()?.exists() {
        return Ok(None);
    }

    let today = today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut session_count = 0usize;
    let mut today_session_count = 0usize;

    for path in dsh::session_transcript_paths()? {
        let file_hours = match dsh::usage_hours(&path) {
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
        SourceApp::Dsh,
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

fn sql_source_days(
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
