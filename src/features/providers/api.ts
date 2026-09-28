import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyProviderInput,
  FetchedModelsResult,
  ModelsDevMatchResult,
  ProviderAppId,
  ProviderAppState,
  ProviderApplyPreview,
  ProviderMutationResult,
  ProviderProtocol,
  ProviderUpsertInput,
  ProvidersState,
} from "./generated";

// 全部 invoke 封装；类型只来自 generated/。明文 API Key 只经
// upsertProvider 的输入下发，单独的密钥读写命令已废弃。

export function getProvidersState(): Promise<ProvidersState> {
  return invoke("get_providers_state");
}

// 单个工具的反显状态：写操作只改一个工具的配置文件，用它做定点刷新，
// 不必重读全部工具。
export function getProviderAppState(app: ProviderAppId): Promise<ProviderAppState> {
  return invoke("get_provider_app_state", { app });
}

// 返回落库后的归一化提供商 ID（小写、`_`→`-`），写密钥等后续调用以它为准。
export function upsertProvider(input: ProviderUpsertInput): Promise<string> {
  return invoke("upsert_provider", { input });
}

export function deleteProvider(providerId: string): Promise<ProviderMutationResult> {
  return invoke("delete_provider", { providerId });
}

export function fetchProviderModels(providerId: string): Promise<FetchedModelsResult> {
  return invoke("fetch_provider_models", { providerId });
}

// 新增提供商未保存时直连拉取；密钥只作当次请求参数，不落盘。
export function fetchProviderModelsDirect(
  protocol: ProviderProtocol,
  baseUrl: string,
  apiKey: string
): Promise<FetchedModelsResult> {
  return invoke("fetch_provider_models_direct", { protocol, baseUrl, apiKey });
}

export function fetchModelsdevCatalog(
  providerId: string,
  modelId: string,
  forceRefresh = false
): Promise<ModelsDevMatchResult> {
  return invoke("fetch_modelsdev_catalog", { providerId, modelId, forceRefresh });
}

export function previewProviderApply(input: ApplyProviderInput): Promise<ProviderApplyPreview> {
  return invoke("preview_provider_apply", { input });
}

export function applyProviderToApp(input: ApplyProviderInput): Promise<ProviderMutationResult> {
  return invoke("apply_provider_to_app", { input });
}

export function removeProviderFromApp(
  providerId: string,
  app: ProviderAppId
): Promise<ProviderMutationResult> {
  return invoke("remove_provider_from_app", { providerId, app });
}

// 外部条目不属于 providers.yaml，按工具配置里的原样键删除；
// Claude 外部配置内嵌在 env 中，后端固定拒绝。
export function removeExternalEntry(
  app: ProviderAppId,
  entryKey: string
): Promise<ProviderMutationResult> {
  return invoke("remove_external_entry", { app, entryKey });
}
