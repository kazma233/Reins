use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// Contract types below cross the Tauri invoke boundary, so ts-rs exports their
// TypeScript bindings straight into the frontend tree (src/features/workspace/
// generated). The path is relative to TS_RS_EXPORT_DIR (./bindings under
// src-tauri), keeping the generated types next to the code that consumes them
// so the hand-written mirrors cannot drift from serde output.

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, TS)]
#[ts(
    export,
    export_to = "../../src/features/workspace/generated/",
    type = "string"
)]
pub(crate) struct AgentTargetId(pub(crate) String);

impl Serialize for AgentTargetId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AgentTargetId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self)
    }
}

impl AgentTargetId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentTargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentTargetId {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        super::normalize_target_id(value)
    }
}

#[cfg(test)]
mod tests {
    use super::AgentTargetId;

    #[test]
    fn serializes_as_a_string() {
        let value = serde_json::to_string(&AgentTargetId("codex".to_string())).unwrap();

        assert_eq!(value, "\"codex\"");
    }

    #[test]
    fn deserializes_from_a_string() {
        let value: AgentTargetId = serde_json::from_str("\"codex\"").unwrap();

        assert_eq!(value.as_str(), "codex");
    }
}

// App-level state returned to frontend. No workspace container anymore.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceState {
    pub(crate) document: Option<WorkspaceConfigDocument>,
    pub(crate) inspection: Option<WorkspaceInspection>,
}

// Generic path picker result used by all file/directory selectors.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceSelection {
    pub(crate) workspace_dir: String,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SkillSourceMutationInput {
    pub(crate) source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) r#ref: Option<String>,
    #[serde(default)]
    pub(crate) include_name_patterns: Vec<String>,
    #[serde(default)]
    pub(crate) include_path_patterns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct BatchGitSkillImportInput {
    pub(crate) yaml_content: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct ValidationReport {
    pub(crate) valid: bool,
    pub(crate) errors: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceConfigDocument {
    pub(crate) config_path: String,
    pub(crate) exists: bool,
    pub(crate) raw_content: String,
    pub(crate) config: Option<WorkspaceConfigView>,
    pub(crate) validation: ValidationReport,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceConfigView {
    pub(crate) targets: Vec<TargetConfigView>,
    pub(crate) skill_sources: Vec<SkillSourceConfigView>,
    pub(crate) mcps: Vec<McpConfigView>,
    pub(crate) projects: Vec<ProjectConfigView>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct TargetConfigView {
    pub(crate) id: AgentTargetId,
    pub(crate) enabled: bool,
    pub(crate) skill_dir: String,
    pub(crate) config_path: Option<String>,
    pub(crate) mcp_config_prefix: String,
    pub(crate) mcp_config_type: McpConfigType,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) enum SkillSourceConfigView {
    Local {
        id: String,
        root_path: String,
        #[serde(default)]
        include_name_patterns: Vec<String>,
        #[serde(default)]
        include_path_patterns: Vec<String>,
        label: String,
    },
    Git {
        id: String,
        repo: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional = nullable)]
        r#ref: Option<String>,
        // 拉取时间来自本地 git cache 的刷新记录，不写入 config.yaml。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(type = "number | null", optional = nullable)]
        last_fetched_at: Option<i64>,
        #[serde(default)]
        include_name_patterns: Vec<String>,
        #[serde(default)]
        include_path_patterns: Vec<String>,
        label: String,
    },
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct McpConfigView {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) transport: McpTransport,
    // i64/u64 travel as JSON numbers over invoke; pin them to `number` instead
    // of ts-rs' default bigint mapping.
    #[ts(type = "number | null")]
    pub(crate) created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) homepage: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) url: Option<String>,
    pub(crate) headers: BTreeMap<String, String>,
    #[ts(type = "number | null")]
    pub(crate) timeout: Option<u64>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct ProjectConfigView {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) agents: Vec<TargetConfigView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) enum McpTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) enum McpConfigType {
    #[serde(rename = "common")]
    Common,
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "grokbuild")]
    GrokBuild,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum McpConfigFileFormat {
    Json,
    Toml,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceInspection {
    pub(crate) config_path: String,
    pub(crate) targets: Vec<TargetInspection>,
    pub(crate) mcps: Vec<McpInspection>,
    pub(crate) projects: Vec<ProjectInspection>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct ProjectInspection {
    pub(crate) id: String,
    pub(crate) path: PathInspection,
    pub(crate) agents: Vec<TargetInspection>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct TargetInspection {
    pub(crate) id: AgentTargetId,
    pub(crate) enabled: bool,
    pub(crate) skill_dir: PathInspection,
    pub(crate) config_path: Option<PathInspection>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct PathInspection {
    pub(crate) path: String,
    pub(crate) exists: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct McpInspection {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) targets: Vec<McpTargetInspection>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct McpTargetInspection {
    pub(crate) target_id: AgentTargetId,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) project_id: Option<String>,
    pub(crate) config_path: Option<String>,
    pub(crate) config_exists: bool,
    pub(crate) state: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SkillSyncItem {
    pub(crate) skill_name: String,
    pub(crate) target_id: AgentTargetId,
    pub(crate) source_path: String,
    pub(crate) destination_path: String,
    pub(crate) action: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceMcpMutationResult {
    pub(crate) server_name: String,
    pub(crate) updated_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct WorkspaceTargetMutationResult {
    pub(crate) target_id: AgentTargetId,
    pub(crate) updated_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct McpTargetMutationResult {
    pub(crate) server_name: String,
    pub(crate) target_id: AgentTargetId,
    pub(crate) updated_path: Option<String>,
    pub(crate) action: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct McpTargetPreviewResult {
    pub(crate) server_name: String,
    pub(crate) target_id: AgentTargetId,
    pub(crate) config_path: String,
    pub(crate) format: String,
    pub(crate) content: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct DiscoveredSkill {
    pub(crate) name: String,
    pub(crate) relative_path: String,
    pub(crate) skill_file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) source_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SkillDiscoveryResultView {
    pub(crate) discovery_id: String,
    pub(crate) repo: String,
    pub(crate) r#ref: Option<String>,
    #[serde(default)]
    pub(crate) include_name_patterns: Vec<String>,
    #[serde(default)]
    pub(crate) include_path_patterns: Vec<String>,
    pub(crate) skills: Vec<DiscoveredSkill>,
    // Skills the include filter would drop, returned alongside so the import
    // preview can show them in a "被排除" section. Empty when no filter is set.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) excluded_skills: Vec<DiscoveredSkill>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BatchGitSkillImportSourceInput {
    pub(crate) repo: String,
    pub(crate) r#ref: String,
    #[serde(default)]
    pub(crate) include_name_patterns: Vec<String>,
    #[serde(default)]
    pub(crate) include_path_patterns: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct BatchGitSkillImportItemResult {
    pub(crate) repo: String,
    pub(crate) r#ref: Option<String>,
    #[serde(default)]
    pub(crate) include_name_patterns: Vec<String>,
    #[serde(default)]
    pub(crate) include_path_patterns: Vec<String>,
    pub(crate) imported_count: usize,
    pub(crate) skill_names: Vec<String>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct BatchGitSkillImportResult {
    pub(crate) imported_count: usize,
    pub(crate) succeeded_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) items: Vec<BatchGitSkillImportItemResult>,
}

// Discovery-time source: git holds a persistent cache checkout (no auto-cleanup).
#[derive(Clone, Debug)]
pub(crate) enum DiscoverySourceDefinition {
    Local { root_path: PathBuf },
    Git { repo: String, r#ref: Option<String> },
}

#[derive(Clone, Debug)]
pub(crate) struct SkillDiscoverySnapshot {
    pub(crate) source: DiscoverySourceDefinition,
    pub(crate) skills: Arc<[DiscoveredSkill]>,
}

// Global app config stored at <config_dir>/reins/config.yaml — user-editable.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawManagerConfig {
    #[serde(default)]
    pub(super) targets: BTreeMap<String, RawTargetConfig>,
    #[serde(default)]
    pub(super) projects: BTreeMap<String, RawProjectConfig>,
    #[serde(default)]
    pub(super) skill_sources: Vec<RawSkillSourceConfig>,
    #[serde(default)]
    pub(super) mcps: Vec<RawMcpConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawProjectConfig {
    pub(super) path: String,
    #[serde(default)]
    pub(super) agents: BTreeMap<String, RawProjectAgentConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawProjectAgentConfig {
    #[serde(default = "super::default_true")]
    pub(super) enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawTargetConfig {
    #[serde(default = "super::default_true")]
    pub(super) enabled: bool,
    pub(super) skill_dir: String,
    #[serde(default)]
    pub(super) mcp: RawTargetMcpConfig,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RawTargetMcpConfig {
    pub(super) config_path: Option<String>,
    pub(super) config_prefix: Option<String>,
    pub(super) config_type: Option<McpConfigType>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RawSkillSourceConfig {
    pub(super) id: String,
    #[serde(flatten)]
    pub(super) source: RawSkillSourceDefinition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum RawSkillSourceDefinition {
    Local {
        root_path: String,
        #[serde(default)]
        include_name_patterns: Vec<String>,
        #[serde(default)]
        include_path_patterns: Vec<String>,
    },
    Git {
        repo: String,
        #[serde(default)]
        r#ref: Option<String>,
        #[serde(default)]
        include_name_patterns: Vec<String>,
        #[serde(default)]
        include_path_patterns: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RawMcpConfig {
    pub(crate) name: String,
    #[serde(default = "super::default_true")]
    pub(crate) enabled: bool,
    pub(crate) transport: McpTransport,
    pub(crate) created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) homepage: Option<String>,
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) timeout: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct RawTargetInput {
    pub(crate) target_id: String,
    pub(crate) enabled: bool,
    pub(crate) skill_dir: String,
    pub(crate) config_path: Option<String>,
    pub(crate) mcp_config_prefix: String,
    pub(crate) mcp_config_type: McpConfigType,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedManagerConfig {
    pub(super) targets: BTreeMap<AgentTargetId, ResolvedTargetConfig>,
    pub(super) projects: BTreeMap<String, ResolvedProjectConfig>,
    pub(super) skill_sources: BTreeMap<String, ResolvedSkillSourceDefinition>,
    pub(super) mcps: Vec<ResolvedMcpConfig>,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedProjectConfig {
    pub(super) id: String,
    pub(super) project_path: PathBuf,
    pub(super) agents: BTreeMap<AgentTargetId, ResolvedTargetConfig>,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedTargetConfig {
    pub(super) id: AgentTargetId,
    pub(super) enabled: bool,
    pub(super) is_project: bool,
    pub(super) skill_dir: PathBuf,
    pub(super) config_path: Option<PathBuf>,
    pub(super) mcp_config_prefix: String,
    pub(super) mcp_config_type: McpConfigType,
}

#[derive(Clone, Debug)]
pub(super) enum ResolvedSkillSourceDefinition {
    Local {
        root_path: PathBuf,
        include_name_patterns: Vec<String>,
        include_path_patterns: Vec<String>,
    },
    Git {
        repo: String,
        r#ref: Option<String>,
        include_name_patterns: Vec<String>,
        include_path_patterns: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedMcpConfig {
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) transport: McpTransport,
    pub(super) created_at: Option<i64>,
    pub(super) homepage: Option<String>,
    pub(super) command: Option<String>,
    pub(super) args: Vec<String>,
    pub(super) env: BTreeMap<String, String>,
    pub(super) url: Option<String>,
    pub(super) headers: BTreeMap<String, String>,
    pub(super) timeout: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct ProjectMutationInput {
    pub(crate) project_id: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) agents: Vec<String>,
}

// Input for the unified sync-a-source-to-targets command.
#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SourceSyncInput {
    pub(crate) source_id: String,
    pub(crate) skill_paths: Vec<String>,
    pub(crate) target_ids: Vec<String>,
    // serde(default) makes these fields omittable on the invoke input, so they
    // stay optional in TS; bool/Vec are not Option, hence the nullable form.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) overwrite_existing: bool,
    #[serde(default)]
    #[ts(optional)]
    pub(crate) source_root: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub(crate) skills: Option<Vec<SyncSkillOption>>,
}

// Output from sync_source_to_targets.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SourceSyncResult {
    pub(crate) removed: Vec<SkillSyncItem>,
    pub(crate) applied: Vec<SkillSyncItem>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SourceSyncConflict {
    pub(crate) skill_name: String,
    pub(crate) target_id: AgentTargetId,
    pub(crate) source_path: String,
    pub(crate) destination_path: String,
    pub(crate) existing_kind: String,
    pub(crate) detail: String,
}

// Ready-to-check list of targets for the sync dialog UI.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SyncTargetOption {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) skill_dir: String,
    pub(crate) enabled: bool,
    // When the skill_dir is a symlink that points at another managed target's
    // skill_dir, this holds that target's id so the UI can disable & hint.
    #[ts(optional = nullable)]
    pub(crate) linked_target_id: Option<String>,
    // 真实 target 目录中扫描到的目录链接。整目录继承的 target 不重复
    // 列出关联，以免把同一批物理链接误认为多份安装。
    pub(crate) links: Vec<SkillLinkAssociation>,
}

// target skills 目录直接子项中发现的目录链接；实际链接而非 config.yaml
// 才是安装状态的权威来源。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SkillLinkAssociation {
    pub(crate) skill_name: String,
    pub(crate) destination_path: String,
    pub(crate) source_path: String,
    // source 根目录可能重叠，文件系统无法证明唯一归属。
    pub(crate) matched_source_ids: Vec<String>,
    pub(crate) state: SkillLinkState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) enum SkillLinkState {
    Linked,
    Excluded,
    SourceMissing,
    Unmanaged,
}

// Ready-to-check list of discoverd skills for the sync dialog UI.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SyncSkillOption {
    pub(crate) name: String,
    pub(crate) relative_path: String,
    // Whether the skill matches the source's include patterns. Skills that
    // don't match are still listed but shown collapsed below the matched ones.
    #[ts(optional = nullable)]
    pub(crate) matched: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/workspace/generated/")]
pub(crate) struct SyncSkillOptionsResult {
    pub(crate) skills: Vec<SyncSkillOption>,
    pub(crate) source_root: String,
}
