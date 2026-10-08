use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tauri::async_runtime;

use crate::logger;

use super::apps::{
    MASKED_KEY, ToolEnv, adapter_for, diff_lines, ensure_protocol_supported, validate_plan,
};
use super::catalog::{fetch_provider_models_inner, match_modelsdev};
use super::config::ProviderConfigStore;
use super::types::{
    ApplyProviderInput, FetchedModelsResult, ModelsDevMatchResult, ProviderAppId, ProviderAppState,
    ProviderApplyPreview, ProviderFilePreview, ProviderModelView, ProviderMutationResult,
    ProviderProtocol, ProviderUpsertInput, ProviderView, ProvidersState, ResolvedProvider,
};
use crate::support::fs::display_path;

// ---------------------------------------------------------------------------
// Tauri commands（薄封装：clone 句柄 + spawn_blocking）
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) async fn get_providers_state(
    store: tauri::State<'_, ProviderConfigStore>,
) -> std::result::Result<ProvidersState, String> {
    let store = store.inner().clone();
    run_blocking(move || {
        let env = ToolEnv::from_env();
        providers_state_inner(&store, &env)
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_provider_app_state(
    store: tauri::State<'_, ProviderConfigStore>,
    app: ProviderAppId,
) -> std::result::Result<ProviderAppState, String> {
    let store = store.inner().clone();
    run_blocking(move || app_state_inner(&store, &ToolEnv::from_env(), app)).await
}

#[tauri::command]
pub(crate) async fn upsert_provider(
    store: tauri::State<'_, ProviderConfigStore>,
    input: ProviderUpsertInput,
) -> std::result::Result<String, String> {
    let store = store.inner().clone();
    logger::log_info(format!("upsert_provider provider_id={}", input.provider_id));
    run_blocking(move || {
        // upsert 会归一化平台 ID（小写、`_`→`-`），并连带写入表单里的明文 API Key。
        store.upsert(input)
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_provider(
    store: tauri::State<'_, ProviderConfigStore>,
    provider_id: String,
) -> std::result::Result<ProviderMutationResult, String> {
    let store = store.inner().clone();
    logger::log_info(format!("delete_provider provider_id={provider_id}"));
    run_blocking(move || delete_provider_inner(&store, &ToolEnv::from_env(), &provider_id)).await
}

// 按表单当前值拉取模型列表：不读 providers.yaml、不写任何持久状态，
// 因此用户改了表单还没保存时，拉取也按改后的值走。
#[tauri::command]
pub(crate) async fn fetch_provider_models_direct(
    protocol: ProviderProtocol,
    base_url: String,
    api_key: String,
) -> std::result::Result<FetchedModelsResult, String> {
    run_blocking(move || fetch_provider_models_direct_inner(&protocol, &base_url, &api_key)).await
}

pub(crate) fn fetch_provider_models_direct_inner(
    protocol: &ProviderProtocol,
    base_url: &str,
    api_key: &str,
) -> Result<FetchedModelsResult> {
    if api_key.trim().is_empty() {
        bail!("请先填写 API Key，再拉取模型列表。");
    }
    if base_url.trim().is_empty() {
        bail!("请先填写 Base URL，再拉取模型列表。");
    }
    let provider = ResolvedProvider {
        id: String::new(),
        label: String::new(),
        protocol: *protocol,
        base_url: base_url.to_string(),
        api_key: None,
        models: Vec::new(),
    };
    fetch_provider_models_inner(&provider, api_key.trim())
}

#[tauri::command]
pub(crate) async fn fetch_modelsdev_catalog(
    store: tauri::State<'_, ProviderConfigStore>,
    provider_id: String,
    model_id: String,
    force_refresh: Option<bool>,
) -> std::result::Result<ModelsDevMatchResult, String> {
    let store = store.inner().clone();
    run_blocking(move || {
        match_modelsdev(
            store.app_dir(),
            &provider_id,
            &model_id,
            force_refresh.unwrap_or(false),
        )
    })
    .await
}

#[tauri::command]
pub(crate) async fn preview_provider_apply(
    store: tauri::State<'_, ProviderConfigStore>,
    input: ApplyProviderInput,
) -> std::result::Result<ProviderApplyPreview, String> {
    let store = store.inner().clone();
    run_blocking(move || preview_apply_inner(&store, &ToolEnv::from_env(), &input)).await
}

#[tauri::command]
pub(crate) async fn apply_provider_to_app(
    store: tauri::State<'_, ProviderConfigStore>,
    input: ApplyProviderInput,
) -> std::result::Result<ProviderMutationResult, String> {
    let store = store.inner().clone();
    logger::log_info(format!(
        "apply_provider_to_app provider_id={} app={:?}",
        input.provider_id, input.app
    ));
    run_blocking(move || apply_provider_inner(&store, &ToolEnv::from_env(), &input)).await
}

#[tauri::command]
pub(crate) async fn remove_provider_from_app(
    store: tauri::State<'_, ProviderConfigStore>,
    provider_id: String,
    app: ProviderAppId,
) -> std::result::Result<ProviderMutationResult, String> {
    let store = store.inner().clone();
    logger::log_info(format!(
        "remove_provider_from_app provider_id={provider_id} app={app:?}"
    ));
    run_blocking(move || {
        remove_provider_from_app_inner(&store, &ToolEnv::from_env(), &provider_id, app)
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_external_entry(
    app: ProviderAppId,
    entry_key: String,
) -> std::result::Result<ProviderMutationResult, String> {
    logger::log_info(format!("remove_external_entry app={app:?} key={entry_key}"));
    run_blocking(move || remove_external_entry_inner(&ToolEnv::from_env(), &app, &entry_key)).await
}

// ---------------------------------------------------------------------------
// Inner implementations（可测试；ToolEnv 由调用方注入）
// ---------------------------------------------------------------------------

pub(crate) fn providers_state_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
) -> Result<ProvidersState> {
    let providers = store.load()?;
    let apps = super::types::PROVIDER_APPS
        .iter()
        .map(|app| inspect_app(&providers, env, *app))
        .collect();

    Ok(ProvidersState {
        config_path: display_path(store.config_path()),
        providers: providers
            .values()
            .map(provider_view)
            .collect::<Result<Vec<_>>>()?,
        apps,
    })
}

// 单个工具的定点反显：写操作只影响一个工具的配置文件，据此只重读该工具，
// 不重读其余工具，也不查密钥（providers.yaml 与密钥都不是写操作的对象）。
pub(crate) fn app_state_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    app: ProviderAppId,
) -> Result<ProviderAppState> {
    Ok(inspect_app(&store.load()?, env, app))
}

fn inspect_app(
    providers: &BTreeMap<String, ResolvedProvider>,
    env: &ToolEnv,
    app: ProviderAppId,
) -> ProviderAppState {
    let adapter = adapter_for(app);
    match adapter.inspect(env, providers) {
        Ok(state) => state,
        Err(error) => {
            // 单个工具配置损坏不拖垮整页；错误挂到对应卡片上。
            let mut state = super::apps::empty_state(app, app_config_paths(env, app));
            state.load_error = Some(error.to_string());
            state
        }
    }
}

pub(crate) fn app_config_paths(env: &ToolEnv, app: ProviderAppId) -> Vec<PathBuf> {
    adapter_for(app).config_paths(env).unwrap_or_default()
}

fn provider_view(provider: &ResolvedProvider) -> Result<ProviderView> {
    Ok(ProviderView {
        id: provider.id.clone(),
        label: provider.label.clone(),
        protocol: provider.protocol,
        base_url: provider.base_url.clone(),
        api_key: provider.stored_api_key(),
        models: provider
            .models
            .iter()
            .map(|model| ProviderModelView {
                id: model.id.clone(),
                label: model.label.clone(),
                context_window: model.context_window,
                max_output_tokens: model.max_output_tokens,
                supports_images: model.supports_images,
                reasoning: model.reasoning,
                reasoning_levels: model.reasoning_levels.clone(),
            })
            .collect(),
    })
}

pub(crate) fn delete_provider_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    provider_id: &str,
) -> Result<ProviderMutationResult> {
    // Claude 的写入没有结构化注册键，平台删掉后就无法安全移除了，
    // 所以删除前强制先从 Claude Code 摘除；其他工具可按 reins- 前缀
    // 结构化移除，不阻断。
    let providers = store.load()?;
    if providers.contains_key(provider_id) {
        let claude_state = adapter_for(ProviderAppId::Claude).inspect(env, &providers)?;
        if claude_state
            .entries
            .iter()
            .any(|entry| entry.provider_id.as_deref() == Some(provider_id))
        {
            let claude_label = ProviderAppId::Claude.label();
            bail!("{claude_label} 仍在引用该平台，请先从 {claude_label} 移除后再删除平台。");
        }
    }

    store.delete(provider_id)?;
    Ok(ProviderMutationResult {
        app: None,
        provider_id: Some(provider_id.to_string()),
        action: "delete".to_string(),
        detail: format!("平台 {provider_id} 已删除。"),
    })
}

// 生成应用产物。preview=true 时内容里的密钥用脱敏占位符，不读真实密钥。
fn compute_apply(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    input: &ApplyProviderInput,
    preview: bool,
) -> Result<(Vec<(PathBuf, String)>, Vec<String>)> {
    let providers = store.load()?;
    let provider = providers
        .get(&input.provider_id)
        .ok_or_else(|| anyhow::anyhow!("平台不存在：{}", input.provider_id))?;
    let adapter = adapter_for(input.app);
    ensure_protocol_supported(adapter, provider)?;
    let warnings = validate_plan(adapter, provider, input)?;

    let Some(stored_key) = provider.stored_api_key() else {
        bail!("请先设置平台 {} 的 API Key。", input.provider_id);
    };
    let api_key = if preview {
        MASKED_KEY.to_string()
    } else {
        stored_key
    };

    let env_files = adapter.apply(env, provider, input, &api_key)?;
    Ok((env_files, warnings))
}

pub(crate) fn preview_apply_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    input: &ApplyProviderInput,
) -> Result<ProviderApplyPreview> {
    let (files, warnings) = compute_apply(store, env, input, true)?;
    // 旧文件可能带着上次 apply 写入的真实密钥；diff 的 Context/Remove 行
    // 只允许出现掩码，用当前密钥值替换。密钥轮换后的历史旧值无从得知，
    // 超出此处的能力边界。
    let secret = store
        .load()?
        .get(&input.provider_id)
        .and_then(|provider| provider.stored_api_key())
        .unwrap_or_default();
    let mut previews = Vec::new();
    for (path, new_content) in files {
        let exists = path.exists();
        // 读失败要让预览失败，而不是把不可读的旧文件当成空文件展示成全量新增。
        let old_content = if exists {
            std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?
        } else {
            String::new()
        };
        let old_content = if secret.is_empty() {
            old_content
        } else {
            old_content.replace(&secret, MASKED_KEY)
        };
        previews.push(ProviderFilePreview {
            path: display_path(&path),
            exists,
            diff: diff_lines(&old_content, &new_content),
        });
    }
    Ok(ProviderApplyPreview {
        app: input.app,
        files: previews,
        warnings,
    })
}

pub(crate) fn apply_provider_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    input: &ApplyProviderInput,
) -> Result<ProviderMutationResult> {
    let (files, warnings) = compute_apply(store, env, input, false)?;
    // 原子写逐个文件执行；中途失败时已写文件保持新内容，靠预览 +
    // 幂等重试收敛，不做跨文件回滚。
    super::apps::write_files(&files)?;
    let written = files
        .iter()
        .map(|(path, _)| display_path(path))
        .collect::<Vec<_>>()
        .join("、");
    let mut detail = format!("已写入：{written}");
    if !warnings.is_empty() {
        detail.push_str(&format!("（{} 条能力警告）", warnings.len()));
    }
    Ok(ProviderMutationResult {
        app: Some(input.app),
        provider_id: Some(input.provider_id.clone()),
        action: "apply".to_string(),
        detail,
    })
}

pub(crate) fn remove_provider_from_app_inner(
    store: &ProviderConfigStore,
    env: &ToolEnv,
    provider_id: &str,
    app: ProviderAppId,
) -> Result<ProviderMutationResult> {
    let providers: BTreeMap<String, ResolvedProvider> = store.load()?;
    let provider = providers.get(provider_id);
    let adapter = adapter_for(app);
    let files = adapter.remove(env, provider_id, provider)?;
    super::apps::write_files(&files)?;
    let written = files
        .iter()
        .map(|(path, _)| display_path(path))
        .collect::<Vec<_>>()
        .join("、");
    Ok(ProviderMutationResult {
        app: Some(app),
        provider_id: Some(provider_id.to_string()),
        action: "remove".to_string(),
        detail: format!("已从 {} 移除：{written}", app.label()),
    })
}

// 外部条目不属于 providers.yaml，按工具配置里的原样键结构化删除；
// 破坏性由前端确认弹窗承担，后端只做键存在性检查；默认模型/活动
// Provider 仍指向被删条目时随删除清空，不再要求先切换。
pub(crate) fn remove_external_entry_inner(
    env: &ToolEnv,
    app: &ProviderAppId,
    entry_key: &str,
) -> Result<ProviderMutationResult> {
    let adapter = adapter_for(*app);
    let files = adapter.remove_external(env, entry_key)?;
    super::apps::write_files(&files)?;
    let written = files
        .iter()
        .map(|(path, _)| display_path(path))
        .collect::<Vec<_>>()
        .join("、");
    Ok(ProviderMutationResult {
        app: Some(*app),
        provider_id: None,
        action: "remove_external".to_string(),
        detail: format!("已从 {} 删除外部条目 {entry_key}：{written}", app.label()),
    })
}

async fn run_blocking<T, F>(operation: F) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    async_runtime::spawn_blocking(move || operation().map_err(|error| error.to_string()))
        .await
        .map_err(|error| error.to_string())?
}
