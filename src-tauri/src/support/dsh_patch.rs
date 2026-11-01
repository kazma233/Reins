// dsh 的 Cordis patch YAML(cordis.patch.yml)操作列表读写。workspace 域
// (MCP 分发)与 providers 域(LLM 路由)写同一层格式,共用哨兵处理与序列化。

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_yaml::Value as YamlValue;

use super::fs::write_atomic;

// serde_yaml 会把双叹号标签(!!js 等 YAML 主命名空间简写)解析成普通标量并
// 丢弃标签,直接往返会静默改变用户条目的运行时语义(!!js process.cwd() 变成
// 字面路径)。读入时换成哨兵单叹号标签保住 Tagged 结构,写出时再还原文本。
// 哨兵字符串本身出现在用户文件里的概率视为零。
const BANGBANG_SENTINEL: &str = "!reins-yaml-bangbang-";

pub(crate) fn read_ops(config_path: &Path) -> Result<Vec<YamlValue>> {
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(config_path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    // !! 标签换哨兵:见 BANGBANG_SENTINEL 注释。
    let normalized = raw.replace("!!", BANGBANG_SENTINEL);
    let parsed: YamlValue = serde_yaml::from_str(&normalized)
        .with_context(|| format!("Cordis patch 解析失败：{}", config_path.display()))?;
    match parsed {
        YamlValue::Null => Ok(Vec::new()),
        YamlValue::Sequence(ops) => Ok(ops),
        _ => bail!(
            "Cordis patch 根节点必须是操作列表：{}",
            config_path.display()
        ),
    }
}

// 序列化为可落盘文本;providers 域用它组装预览/写入内容,不落盘。
pub(crate) fn serialize_ops(ops: &[YamlValue]) -> Result<String> {
    let serialized =
        serde_yaml::to_string(ops).with_context(|| "Cordis patch 序列化失败".to_string())?;
    // 哨兵标签还原回 !! 简写:见 BANGBANG_SENTINEL 注释。
    Ok(serialized.replace(BANGBANG_SENTINEL, "!!"))
}

pub(crate) fn write_ops(config_path: &Path, ops: &[YamlValue]) -> Result<()> {
    write_ops_content(config_path, &serialize_ops(ops)?)
}

pub(crate) fn write_ops_content(config_path: &Path, contents: &str) -> Result<()> {
    write_atomic(config_path, contents)
        .with_context(|| format!("写入 Cordis patch 失败：{}", config_path.display()))
}

// 展示用途的标签剥离:标签对 JSON 无意义,递归取 Tagged 内层值,否则带 !!js
// 的用户条目在 preview 里会以怪异形状出现或转换失败。
pub(crate) fn untag(value: &YamlValue) -> YamlValue {
    match value {
        YamlValue::Tagged(tagged) => untag(&tagged.value),
        YamlValue::Sequence(sequence) => YamlValue::Sequence(sequence.iter().map(untag).collect()),
        YamlValue::Mapping(mapping) => YamlValue::Mapping(
            mapping
                .iter()
                .map(|(key, value)| (key.clone(), untag(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}
