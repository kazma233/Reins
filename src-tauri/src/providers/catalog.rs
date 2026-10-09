// 模型目录：按平台协议拉取模型列表 + models.dev 元数据补全（带缓存）。
// 网络请求只在显式触发时发生，固定超时；API Key 只用于当次请求，
// 不写入日志、错误或缓存。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value as JsonValue;

use super::types::{
    FetchedModel, FetchedModelsResult, ModelsDevCandidate, ModelsDevMatchResult,
    ModelsDevMatchStatus, ModelsDevMeta, ReasoningLevel, ResolvedProvider,
};
use crate::logger;
use crate::support::fs::write_atomic;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_FILE_NAME: &str = "modelsdev-cache.json";
const CACHE_TTL_MS: i64 = 24 * 60 * 60 * 1000;

// 聚合平台模型列表：GET {base_url}/models。兼容两种返回形态：
// { "data": [ { "id": ... } ] }（OpenAI/Anthropic 官方形态）与直接数组。
pub(crate) fn fetch_provider_models_inner(
    provider: &ResolvedProvider,
    api_key: &str,
) -> Result<FetchedModelsResult> {
    // 拉取路径按 base_url 的生态约定逐级回退：
    // 1. {base}/v1/models —— Anthropic 官方约定（官方、智谱兼容层实测存在）；
    // 2. {base}/models —— 部分兼容层不挂版本段；
    // 3. 去掉 /anthropic 子路径后的 /models —— 兼容层普遍挂 /anthropic
    //    子路径（DeepSeek 文档约定），其 OpenAI 形态模型列表在根路径，
    //    该跳使用 Bearer 头。仅在 404 / 明确不是模型列表时继续下一候选，
    //    其他失败（鉴权等）直接报，避免掩盖真实原因。
    let base = provider.base_url.trim_end_matches('/');
    let mut candidates: Vec<(String, bool)> = if provider.protocol.uses_api_key_header() {
        if base.ends_with("/v1") {
            vec![(format!("{base}/models"), false)]
        } else {
            vec![
                (format!("{base}/v1/models"), false),
                (format!("{base}/models"), false),
            ]
        }
    } else {
        vec![(format!("{base}/models"), true)]
    };
    if let Some(root) = base.strip_suffix("/anthropic") {
        candidates.push((format!("{root}/models"), true));
    }

    let client = build_client()?;
    for (index, (url, bearer)) in candidates.iter().enumerate() {
        let mut request = client.get(url);
        if *bearer {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        } else {
            request = request
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }

        let response = request
            .send()
            .with_context(|| format!("请求模型列表失败：{url}"))?;
        let status = response.status();
        // 错误响应体可能回显请求内容，只保留状态码，不落响应体。
        if !status.is_success() {
            let has_next = index + 1 < candidates.len();
            logger::log_info(format!(
                "fetch_models candidate failed url={url} status={status} try_next={has_next}"
            ));
            // 仅资源不存在时尝试下一个候选路径。
            if status.as_u16() == 404 && has_next {
                continue;
            }
            bail!("模型列表请求失败：HTTP {status}");
        }
        // 先取原始字节再解析：2xx 但不是 JSON 时要把响应体开头带出来，
        // 否则只报「不是 JSON」看不出网关到底回了什么（HTML 错误页 / 纯文本 404）。
        // 4xx/5xx 仍然不落响应体，那条路径可能回显请求头里的 Key。
        let raw = response
            .bytes()
            .with_context(|| format!("读取模型列表响应失败：{url}"))?;
        let body: JsonValue = match serde_json::from_slice(&raw) {
            Ok(value) => value,
            Err(_) => bail!(
                "模型列表返回的不是 JSON：{url}：{}",
                describe_non_json_body(&raw)
            ),
        };
        match parse_models_payload(&body) {
            Ok(models) => {
                return Ok(FetchedModelsResult {
                    url: url.clone(),
                    models,
                });
            }
            Err(error) => {
                // 部分网关把 404 包在 HTTP 200 body 里（如智谱
                // {"code":500,"msg":"404 NOT_FOUND"}），同样视为候选路径
                // 不存在，继续尝试下一跳；其余解析错误直接返回。
                let has_next = index + 1 < candidates.len();
                logger::log_info(format!(
                    "fetch_models candidate not a model list url={url} try_next={has_next}"
                ));
                if matches!(error, PayloadError::MissingData(_)) && has_next {
                    continue;
                }
                return Err(error.into());
            }
        }
    }
    unreachable!("候选路径列表非空，循环内必然返回")
}

// 非 JSON 响应体的展示形态：压成单行并截断到 1000 字符，
// 足够看清网关返回了什么，又不会把整页内容塞进错误串。
fn describe_non_json_body(raw: &[u8]) -> String {
    const MAX_CHARS: usize = 1000;
    let text = String::from_utf8_lossy(raw);
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        return "（响应体为空）".to_string();
    }
    let mut head: String = flattened.chars().take(MAX_CHARS).collect();
    if flattened.chars().count() > MAX_CHARS {
        head.push('…');
    }
    head
}

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .context("无法创建 HTTP 客户端")
}

// 模型列表 payload 的解析错误。MissingData 专指「HTTP 200 但 body 不是
// 模型列表」（网关把业务错误包在 body 里），它是候选路径回退的判断依据；
// 回退与否不能依赖错误文案，改文案不会改变回退行为。
#[derive(Debug)]
enum PayloadError {
    MissingData(String),
    Unrecognized,
}

impl std::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::MissingData(detail) => {
                write!(f, "模型列表返回缺少 data 字段：{detail}")
            }
            PayloadError::Unrecognized => write!(f, "模型列表返回格式无法识别。"),
        }
    }
}

impl std::error::Error for PayloadError {}

fn parse_models_payload(body: &JsonValue) -> Result<Vec<FetchedModel>, PayloadError> {
    let items = match body {
        JsonValue::Array(items) => items,
        JsonValue::Object(map) => match map.get("data").and_then(|data| data.as_array()) {
            Some(items) => items,
            // 聚合平台网关常用 HTTP 200 + 业务错误对象（如智谱
            // {"success":false,"code":...,"msg":...}），状态码检查拦不住；
            // 把上游错误字段带出来，避免「缺少 data」掩盖真实失败原因。
            None => {
                return Err(PayloadError::MissingData(describe_upstream_error(map)));
            }
        },
        _ => return Err(PayloadError::Unrecognized),
    };

    let mut models = Vec::new();
    for item in items {
        let Some(id) = item.get("id").and_then(JsonValue::as_str) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let name = item
            .get("display_name")
            .or_else(|| item.get("name"))
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        models.push(FetchedModel {
            id: id.to_string(),
            name,
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

// 提取 200 型错误对象里的常见错误字段：code 为数字或字符串，错误文本
// 优先 msg/message，兼容 OpenAI 的嵌套 error.message。只做展示，不做
// 厂商语义判断。
fn describe_upstream_error(map: &serde_json::Map<String, JsonValue>) -> String {
    let mut parts = Vec::new();
    // code 顶层（智谱形态）或嵌套在 error 对象里（OpenAI 形态）都取。
    match map
        .get("code")
        .or_else(|| map.get("error").and_then(|error| error.get("code")))
    {
        Some(JsonValue::Number(code)) => parts.push(format!("code={code}")),
        Some(JsonValue::String(code)) if !code.is_empty() => parts.push(format!("code={code}")),
        _ => {}
    }
    let text = ["msg", "message"]
        .iter()
        .find_map(|key| map.get(*key).and_then(JsonValue::as_str))
        .map(str::to_string)
        .or_else(|| match map.get("error") {
            Some(JsonValue::String(text)) => Some(text.clone()),
            Some(JsonValue::Object(error)) => error
                .get("message")
                .and_then(JsonValue::as_str)
                .map(str::to_string),
            _ => None,
        });
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        // 上游错误文本长度不可控，截断避免错误提示刷屏。
        const MAX_CHARS: usize = 200;
        if text.chars().count() > MAX_CHARS {
            parts.push(text.chars().take(MAX_CHARS).collect::<String>() + "…");
        } else {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        "无法从返回中识别错误原因。".to_string()
    } else {
        parts.join("，")
    }
}

// ---------------------------------------------------------------------------
// models.dev
// ---------------------------------------------------------------------------

// models.dev 的 reasoning 取值归一化；未知取值丢弃而不是猜测。
pub(crate) fn normalize_reasoning_levels(values: &[String]) -> Vec<ReasoningLevel> {
    let mut levels: Vec<ReasoningLevel> = values
        .iter()
        .filter_map(|value| ReasoningLevel::parse(value))
        .collect();
    levels.sort();
    levels.dedup();
    levels
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModelsDevCache {
    fetched_at_ms: i64,
    data: JsonValue,
}

fn cache_path(app_dir: &Path) -> PathBuf {
    app_dir.join(CACHE_FILE_NAME)
}

fn load_modelsdev_data(app_dir: &Path, force_refresh: bool) -> Result<JsonValue> {
    let path = cache_path(app_dir);
    if !force_refresh {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cache) = serde_json::from_str::<ModelsDevCache>(&content) {
                if now_ms() - cache.fetched_at_ms < CACHE_TTL_MS {
                    return Ok(cache.data);
                }
            }
        }
    }

    let client = build_client()?;
    let response = client
        .get("https://models.dev/api.json")
        .send()
        .context("拉取 models.dev 目录失败")?;
    if !response.status().is_success() {
        bail!("models.dev 请求失败：HTTP {}", response.status());
    }
    let data: JsonValue = response.json().context("models.dev 返回的不是 JSON")?;

    let cache = ModelsDevCache {
        fetched_at_ms: now_ms(),
        data: data.clone(),
    };
    let serialized = serde_json::to_string(&cache)?;
    // 缓存写失败只影响下次复用，不阻断本次补全；write_atomic 自带建目录。
    let _ = write_atomic(&path, &serialized);
    Ok(data)
}

pub(crate) fn match_modelsdev(
    app_dir: &Path,
    provider_id: &str,
    model_id: &str,
    force_refresh: bool,
) -> Result<ModelsDevMatchResult> {
    let data = load_modelsdev_data(app_dir, force_refresh)?;
    let catalog = parse_modelsdev_catalog(&data);
    Ok(match_modelsdev_in_catalog(&catalog, provider_id, model_id))
}

// 解析后的目录：provider 名（小写）→ 模型 id → (label, meta)。
type ModelsDevCatalog = BTreeMap<String, BTreeMap<String, (Option<String>, ModelsDevMeta)>>;

fn parse_modelsdev_catalog(data: &JsonValue) -> ModelsDevCatalog {
    let mut catalog = ModelsDevCatalog::new();
    let Some(providers) = data.as_object() else {
        return catalog;
    };
    for (provider_name, provider_value) in providers {
        let Some(models) = provider_value.get("models").and_then(JsonValue::as_object) else {
            continue;
        };
        let entry = catalog.entry(provider_name.to_lowercase()).or_default();
        for (model_id, model_value) in models {
            let meta = parse_modelsdev_meta(model_value);
            let label = model_value
                .get("name")
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            entry.insert(model_id.clone(), (label, meta));
        }
    }
    catalog
}

fn parse_modelsdev_meta(model_value: &JsonValue) -> ModelsDevMeta {
    let limit = model_value.get("limit");
    let context_window = limit
        .and_then(|limit| limit.get("context"))
        .and_then(JsonValue::as_i64);
    let max_output_tokens = limit
        .and_then(|limit| limit.get("output"))
        .and_then(JsonValue::as_i64);
    let supports_images = model_value
        .get("modalities")
        .and_then(|modalities| modalities.get("input"))
        .and_then(JsonValue::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(JsonValue::as_str)
                .any(|v| v == "image")
        });
    let reasoning = model_value.get("reasoning").and_then(JsonValue::as_bool);
    // reasoning_options 形态 unverified：兼容 [{values: [...] }] 与
    // [string] 两种，识别不了的取值直接丢弃。
    let reasoning_levels = model_value
        .get("reasoning_options")
        .and_then(JsonValue::as_array)
        .map(|options| {
            let mut values = Vec::new();
            for option in options {
                match option {
                    JsonValue::String(value) => values.push(value.clone()),
                    JsonValue::Object(map) => {
                        if let Some(inner) = map.get("values").and_then(JsonValue::as_array) {
                            for value in inner {
                                if let Some(text) = value.as_str() {
                                    values.push(text.to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            normalize_reasoning_levels(&values)
        })
        .filter(|levels| !levels.is_empty());

    ModelsDevMeta {
        context_window,
        max_output_tokens,
        supports_images,
        reasoning,
        reasoning_levels,
    }
}

// 匹配规则：模型 id 去掉 provider/ 前缀后优先在同名 provider 下精确
// 匹配；否则全库按 id 搜索，多候选交由用户选择。
fn match_modelsdev_in_catalog(
    catalog: &ModelsDevCatalog,
    provider_id: &str,
    model_id: &str,
) -> ModelsDevMatchResult {
    let bare_id = model_id
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(model_id);
    // 目录里可能以完整 id 或裸 id 作键，两种都尝试。
    let lookup_ids: [String; 2] = [model_id.to_string(), bare_id.to_string()];

    let provider_key = provider_id.to_lowercase();
    if let Some(provider_models) = catalog.get(&provider_key) {
        for lookup_id in &lookup_ids {
            if let Some((label, meta)) = provider_models.get(lookup_id) {
                return ModelsDevMatchResult {
                    status: ModelsDevMatchStatus::Exact,
                    meta: Some(meta.clone()),
                    candidates: vec![candidate(&provider_key, lookup_id, label, meta)],
                };
            }
        }
    }

    let mut candidates = Vec::new();
    for (provider_name, models) in catalog {
        for lookup_id in &lookup_ids {
            if let Some((label, meta)) = models.get(lookup_id) {
                candidates.push(candidate(provider_name, lookup_id, label, meta));
                break;
            }
        }
    }
    if candidates.len() == 1 {
        let only = candidates.remove(0);
        return ModelsDevMatchResult {
            status: ModelsDevMatchStatus::Exact,
            meta: Some(only.meta.clone()),
            candidates: vec![only],
        };
    }
    if candidates.is_empty() {
        return ModelsDevMatchResult {
            status: ModelsDevMatchStatus::NotFound,
            meta: None,
            candidates: Vec::new(),
        };
    }
    candidates.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.model_id.cmp(&b.model_id))
    });
    ModelsDevMatchResult {
        status: ModelsDevMatchStatus::Candidates,
        meta: None,
        candidates,
    }
}

fn candidate(
    provider: &str,
    model_id: &str,
    label: &Option<String>,
    meta: &ModelsDevMeta,
) -> ModelsDevCandidate {
    ModelsDevCandidate {
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        label: label.clone(),
        meta: meta.clone(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/providers_catalog.rs"]
mod tests;
