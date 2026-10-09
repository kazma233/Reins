//! Shared family timeline aggregation layer for the session readers.
//!
//! The family-index engine owns grouping and ordering; this module owns the
//! family-level summary post-processing and the agent roster. The load-through
//! timeline cache used to live here; it moved one layer up into the reader
//! engine (reader_engine.rs), which holds the caches per instance.

use std::collections::HashSet;

use super::family_index::{Family, FamilyRow};
use super::{SessionAgent, SessionMessage, SessionSummary, SessionTokenUsage};

impl<Row: FamilyRow> Family<Row> {
    /// 会话条目的 token 统计口径:family 全部成员(root、resume 段、子代理
    /// 线程)的消耗相加才是整个会话的用量;成员没有 usage 数据时跳过。
    pub(crate) fn sum_token_usage(
        &self,
        usage_of: impl Fn(&Row) -> Option<SessionTokenUsage>,
    ) -> Option<SessionTokenUsage> {
        let mut total = None;
        for row in &self.members {
            if let Some(usage) = usage_of(row) {
                super::merge_token_usage(&mut total, usage);
            }
        }
        total
    }

    /// Fold a root-member summary into the family-level shape shared by the
    /// file-backed backends: the "(+N subagents)" title, the root transcript
    /// path, and family-wide timestamps. OpenCode builds its summary straight
    /// from a database row and never goes through this shape.
    pub(crate) fn apply_summary_aggregates(&self, summary: &mut SessionSummary) {
        // 只统计 id 与 root 不同的成员：Codex 的 resume 续跑段与原始段共享
        // session id，是同一条线程而不是 subagent，不能按 members.len() - 1 计。
        let root_id = self.root.member_id();
        let child_count = self
            .members
            .iter()
            .filter(|row| row.member_id() != root_id)
            .count();

        if child_count > 0 {
            summary.title = format!("{} (+{} subagents)", summary.title, child_count);
        }

        summary.transcript_path = self.root.member_path().display().to_string();
        summary.created_at = self.created_at().or(summary.created_at);
        summary.updated_at = self.updated_at();
    }
}

/// 子代理弹窗的数据口径：family 消息里按 session id 归属于该子代理的部分
/// （marker 消息与子会话消息同 session id）。
pub(crate) fn agent_messages(
    messages: Vec<SessionMessage>,
    agent_session_id: &str,
) -> Vec<SessionMessage> {
    messages
        .into_iter()
        .filter(|message| message.session_id.as_deref() == Some(agent_session_id))
        .collect()
}

/// Label policy a backend supplies per member. The roster builder derives
/// `is_root` from the variant so the flag and the displayed label can never
/// disagree.
pub(crate) enum FamilyAgentLabel {
    Root,
    Child(String),
    /// Codex forks carry a source thread id and are shown as derived rather
    /// than spawned.
    Derived(String),
}

pub(crate) fn family_agents<Row: FamilyRow>(
    family: &Family<Row>,
    label_of: impl Fn(&Row) -> FamilyAgentLabel,
) -> Vec<SessionAgent> {
    // Codex 的 resume 段与原始段都标为 Root 且共享 member id；root 先登记，
    // 可以去掉续跑段，同时保留 Claude 中“子代理回退使用 family key”的条目。
    let mut seen_root_ids = HashSet::new();
    std::iter::once(&family.root)
        .chain(family.members.iter())
        .filter_map(|row| {
            let label = label_of(row);
            if matches!(label, FamilyAgentLabel::Root)
                && !seen_root_ids.insert(row.member_id().to_string())
            {
                return None;
            }

            Some((row, label))
        })
        .map(|row| {
            let (row, label) = row;
            let (label, is_root) = match label {
                FamilyAgentLabel::Root => ("主 Agent".to_string(), true),
                FamilyAgentLabel::Child(name) => (format!("{name}(子)"), false),
                FamilyAgentLabel::Derived(name) => (format!("{name}(派生)"), false),
            };

            SessionAgent {
                session_id: row.member_id().to_string(),
                label,
                is_root,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    use super::*;

    #[derive(Clone, Debug)]
    struct TestRow {
        id: String,
        path: PathBuf,
        created_at: Option<i64>,
        updated_at: Option<i64>,
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
            self.created_at
        }

        fn member_updated_at(&self) -> Option<i64> {
            self.updated_at
        }

        fn member_tie_breaker(&self) -> Cow<'_, str> {
            Cow::Borrowed(&self.id)
        }
    }

    fn family_with_children() -> Family<TestRow> {
        Family {
            root: TestRow {
                id: "root".to_string(),
                path: PathBuf::from("/tmp/root.jsonl"),
                created_at: Some(100),
                updated_at: Some(500),
            },
            members: vec![
                TestRow {
                    id: "root".to_string(),
                    path: PathBuf::from("/tmp/root.jsonl"),
                    created_at: Some(100),
                    updated_at: Some(500),
                },
                TestRow {
                    id: "child".to_string(),
                    path: PathBuf::from("/tmp/child.jsonl"),
                    created_at: Some(200),
                    updated_at: Some(600),
                },
            ],
        }
    }

    fn summary(title: &str) -> SessionSummary {
        SessionSummary {
            source_app: super::super::SourceApp::ClaudeCode,
            source_session_id: "root".to_string(),
            title: title.to_string(),
            cwd: None,
            git_branch: None,
            transcript_path: String::new(),
            created_at: None,
            updated_at: Some(1),
            token_usage: None,
        }
    }

    #[test]
    fn summary_aggregates_append_child_count_and_family_timestamps() {
        let family = family_with_children();
        let mut summary = summary("Root title");

        family.apply_summary_aggregates(&mut summary);

        assert_eq!(summary.title, "Root title (+1 subagents)");
        assert_eq!(summary.transcript_path, "/tmp/root.jsonl");
        assert_eq!(summary.created_at, Some(100));
        assert_eq!(summary.updated_at, Some(600));
    }

    #[test]
    fn summary_aggregates_keep_title_without_children_and_fallback_created_at() {
        let family = Family {
            root: TestRow {
                id: "root".to_string(),
                path: PathBuf::from("/tmp/root.jsonl"),
                created_at: None,
                updated_at: None,
            },
            members: vec![TestRow {
                id: "root".to_string(),
                path: PathBuf::from("/tmp/root.jsonl"),
                created_at: None,
                updated_at: None,
            }],
        };
        let mut summary = summary("Solo title");

        family.apply_summary_aggregates(&mut summary);

        assert_eq!(summary.title, "Solo title");
        // Family timestamps are absent, so the root summary values survive.
        assert_eq!(summary.created_at, None);
        assert_eq!(summary.updated_at, None);
    }

    #[test]
    fn agent_roster_uses_member_ids_and_label_variants() {
        let mut forked = family_with_children();
        forked.members.push(TestRow {
            id: "root".to_string(),
            path: PathBuf::from("/tmp/root-resume.jsonl"),
            created_at: Some(150),
            updated_at: Some(550),
        });
        forked.members.push(TestRow {
            id: "root".to_string(),
            path: PathBuf::from("/tmp/root-child.jsonl"),
            created_at: Some(175),
            updated_at: Some(575),
        });
        forked.members.push(TestRow {
            id: "fork".to_string(),
            path: PathBuf::from("/tmp/fork.jsonl"),
            created_at: Some(300),
            updated_at: Some(700),
        });

        let root_id = |family: &Family<TestRow>| family.root.id.clone();
        let roster = family_agents(&forked, |row| {
            if row.path.ends_with("root-child.jsonl") {
                FamilyAgentLabel::Child("Fallback child".to_string())
            } else if row.id == root_id(&forked) {
                FamilyAgentLabel::Root
            } else if row.id == "fork" {
                FamilyAgentLabel::Derived("Fork".to_string())
            } else {
                FamilyAgentLabel::Child("Child".to_string())
            }
        });

        let as_tuples = roster
            .iter()
            .map(|agent| {
                (
                    agent.session_id.as_str(),
                    agent.label.as_str(),
                    agent.is_root,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            as_tuples,
            vec![
                ("root", "主 Agent", true),
                ("child", "Child(子)", false),
                ("root", "Fallback child(子)", false),
                ("fork", "Fork(派生)", false),
            ]
        );
    }
}
