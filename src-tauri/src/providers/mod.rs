// providers 域：聚合平台模型管理。与 session、workspace 互不依赖；
// 持久化在独立的 providers.yaml（含明文 API Key），工具配置写入
// 由 apps/ 下的适配器完成。

pub(crate) mod apps;
pub(crate) mod catalog;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod types;

pub(crate) use config::ProviderConfigStore;

#[cfg(test)]
#[path = "../tests/providers.rs"]
mod tests;
