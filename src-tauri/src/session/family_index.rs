//! Shared family-index engine for the session backends.
//!
//! Backends enumerate their own rows and group them into families — the
//! grouping strategies genuinely differ per source app (Claude groups by the
//! shared transcript sessionId, Codex walks the parent-thread chain, OpenCode
//! joins on SQL parent_id), so grouping stays out. Everything downstream of
//! grouping lives here exactly once: member and family ordering, the
//! path-keyed family lookup, and the optional id-keyed dual-write map.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Row contract the engine needs from a backend.
pub(crate) trait FamilyRow {
    /// Transcript path of the row. Borrowed for file-backed backends, derived
    /// from the session id for OpenCode (its rows come from SQLite and have
    /// no path field).
    fn member_path(&self) -> Cow<'_, Path>;

    /// Id the family root is resolvable by — the root's own source id, NOT
    /// the grouping key: an orphan Codex family falls back to a member as
    /// root, and only that root's own id may claim the family entry.
    fn family_root_id(&self) -> &str;

    /// Id that resolves to this member's own file (agent_session_id for
    /// Claude, source_session_id for Codex).
    fn member_id(&self) -> &str;

    fn member_created_at(&self) -> Option<i64>;

    fn member_updated_at(&self) -> Option<i64>;

    /// Secondary member-sort key (root path spelling for Claude, session id
    /// otherwise).
    fn member_tie_breaker(&self) -> Cow<'_, str>;
}

#[derive(Clone, Debug)]
pub(crate) struct Family<Row> {
    pub(crate) root: Row,
    pub(crate) members: Vec<Row>,
}

impl<Row: FamilyRow> Family<Row> {
    pub(crate) fn created_at(&self) -> Option<i64> {
        self.members
            .iter()
            .filter_map(FamilyRow::member_created_at)
            .min()
    }

    pub(crate) fn updated_at(&self) -> Option<i64> {
        self.members
            .iter()
            .filter_map(FamilyRow::member_updated_at)
            .max()
    }

    /// Transcript paths of all members, root included, in member order.
    /// Backends with an extra non-member source (the OpenCode database file)
    /// prepend it themselves.
    pub(crate) fn source_paths(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|row| row.member_path().display().to_string())
            .collect()
    }
}

#[derive(Clone)]
pub(crate) struct FamilyIndex<Row> {
    pub(crate) families: Vec<Family<Row>>,
    pub(crate) sessions_by_path: HashMap<String, Family<Row>>,
    /// `None` marks a backend without id resolution (OpenCode resolves ids
    /// directly from the database instead).
    pub(crate) sessions_by_id: Option<HashMap<String, PathBuf>>,
}

impl<Row: FamilyRow + Clone> FamilyIndex<Row> {
    /// Index without id resolution.
    pub(crate) fn build(families: Vec<Family<Row>>) -> Self {
        let mut families = families;
        sort_members(&mut families);
        sort_families(&mut families);

        let mut sessions_by_path = HashMap::new();

        for family in &families {
            for member in &family.members {
                sessions_by_path.insert(
                    crate::support::fs::path_key(member.member_path().as_ref()),
                    family.clone(),
                );
            }
        }

        Self {
            families,
            sessions_by_path,
            sessions_by_id: None,
        }
    }

    /// Index with the dual-write id map: the family key resolves to the root
    /// transcript (first claim wins), each member id to the member's own
    /// file.
    pub(crate) fn build_with_ids(families: Vec<Family<Row>>) -> Self {
        let mut index = Self::build(families);
        let mut sessions_by_id = HashMap::new();

        for family in &index.families {
            for member in &family.members {
                sessions_by_id
                    .entry(family.root.family_root_id().to_string())
                    .or_insert_with(|| family.root.member_path().into_owned());
                sessions_by_id.insert(
                    member.member_id().to_string(),
                    member.member_path().into_owned(),
                );
            }
        }

        index.sessions_by_id = Some(sessions_by_id);
        index
    }

    pub(crate) fn path_for_id(&self, source_session_id: &str) -> Option<PathBuf> {
        self.sessions_by_id
            .as_ref()?
            .get(source_session_id)
            .cloned()
    }
}

// Rows arrive in WalkDir enumeration order, which is not guaranteed across
// runs. Every first-claim-wins insert below depends on member order, so
// members must be fully sorted before the maps are built for the index to be
// a pure function of its input.
fn sort_members<Row: FamilyRow>(families: &mut [Family<Row>]) {
    for family in families {
        family.members.sort_by_cached_key(|row| {
            (
                row.member_created_at(),
                row.member_tie_breaker().into_owned(),
            )
        });
    }
}

// Family order feeds nothing order-sensitive — list_entries re-sorts its
// output — but keeping it deterministic keeps the id map deterministic too.
fn sort_families<Row: FamilyRow>(families: &mut [Family<Row>]) {
    families.sort_by(|left, right| {
        right
            .updated_at()
            .cmp(&left.updated_at())
            .then_with(|| left.root.member_path().cmp(&right.root.member_path()))
    });
}

#[derive(Clone)]
pub(crate) struct FamilyIndexCacheEntry<Row> {
    pub(crate) source_key: String,
    pub(crate) updated_at: i64,
    pub(crate) index: FamilyIndex<Row>,
}

impl<Row> FamilyIndexCacheEntry<Row> {
    pub(crate) fn is_valid(&self, source_key: &str, updated_at: i64) -> bool {
        self.source_key == source_key && self.updated_at == updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestRow {
        id: String,
        path: PathBuf,
        created_at: Option<i64>,
        updated_at: Option<i64>,
    }

    impl TestRow {
        fn new(id: &str, path: &str, created_at: i64, updated_at: i64) -> Self {
            Self {
                id: id.to_string(),
                path: PathBuf::from(path),
                created_at: Some(created_at),
                updated_at: Some(updated_at),
            }
        }
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

    fn sample_families() -> Vec<Family<TestRow>> {
        vec![
            Family {
                root: TestRow::new("root-a", "/tmp/a/root.jsonl", 100, 500),
                members: vec![
                    TestRow::new("root-a", "/tmp/a/root.jsonl", 100, 500),
                    TestRow::new("child-a1", "/tmp/a/child-1.jsonl", 200, 600),
                    TestRow::new("child-a2", "/tmp/a/child-2.jsonl", 200, 400),
                ],
            },
            Family {
                root: TestRow::new("root-b", "/tmp/b/root.jsonl", 50, 900),
                members: vec![TestRow::new("root-b", "/tmp/b/root.jsonl", 50, 900)],
            },
        ]
    }

    #[test]
    fn dual_write_resolves_family_key_to_root_and_member_id_to_member() {
        let index = FamilyIndex::build_with_ids(sample_families());

        assert_eq!(
            index.path_for_id("root-a").as_deref(),
            Some(Path::new("/tmp/a/root.jsonl"))
        );
        assert_eq!(
            index.path_for_id("child-a1").as_deref(),
            Some(Path::new("/tmp/a/child-1.jsonl"))
        );
        assert_eq!(
            index.path_for_id("child-a2").as_deref(),
            Some(Path::new("/tmp/a/child-2.jsonl"))
        );
        assert_eq!(index.path_for_id("missing"), None);
    }

    #[test]
    fn member_id_equal_to_family_key_claims_its_own_file() {
        // Claude's agent_session_id falls back to the family key when the
        // transcript file is not named agent-*; the member insert then
        // overrides the family entry with the member's own path.
        let families = vec![Family {
            root: TestRow::new("shared", "/tmp/a/root.jsonl", 100, 500),
            members: vec![
                TestRow::new("shared", "/tmp/a/root.jsonl", 100, 500),
                TestRow::new("shared", "/tmp/a/unnamed.jsonl", 200, 600),
            ],
        }];

        let index = FamilyIndex::build_with_ids(families);

        // Sorted members put the later-created member last, so its insert
        // wins the contested key.
        assert_eq!(
            index.path_for_id("shared").as_deref(),
            Some(Path::new("/tmp/a/unnamed.jsonl"))
        );
    }

    #[test]
    fn index_is_deterministic_across_input_orders() {
        let mut families = sample_families();
        families[0].members.reverse();

        let index = FamilyIndex::build_with_ids(sample_families());
        let shuffled = FamilyIndex::build_with_ids(families);

        let to_ids = |index: &FamilyIndex<TestRow>| {
            index
                .families
                .iter()
                .flat_map(|family| family.members.iter().map(|row| row.id.clone()))
                .collect::<Vec<_>>()
        };

        assert_eq!(to_ids(&index), to_ids(&shuffled));
        assert_eq!(index.sessions_by_id, shuffled.sessions_by_id);
        // Equal created_at + distinct tie-breakers must order by the tie key
        // (families themselves sort updated_at desc, so root-b comes first).
        assert_eq!(to_ids(&index)[1..], ["root-a", "child-a1", "child-a2"]);
    }

    #[test]
    fn build_without_ids_has_no_id_map() {
        let index = FamilyIndex::build(sample_families());

        assert!(index.sessions_by_id.is_none());
        assert_eq!(index.path_for_id("root-a"), None);

        let key = crate::support::fs::path_key(Path::new("/tmp/a/child-1.jsonl"));
        let family = index
            .sessions_by_path
            .get(&key)
            .expect("path map still resolves members");
        assert_eq!(family.root.id, "root-a");
    }

    #[test]
    fn families_sort_by_updated_at_desc_then_root_path() {
        let index = FamilyIndex::build(sample_families());

        let order = index
            .families
            .iter()
            .map(|family| family.root.id.clone())
            .collect::<Vec<_>>();

        // root-b family (updated 900) outranks root-a family (updated 600).
        assert_eq!(order, ["root-b", "root-a"]);
    }

    #[test]
    fn source_paths_list_members_in_member_order() {
        let index = FamilyIndex::build(sample_families());

        // Families sort updated_at desc, so the root-a family sits second.
        let paths = index.families[1].source_paths();

        assert_eq!(
            paths,
            [
                "/tmp/a/root.jsonl".to_string(),
                "/tmp/a/child-1.jsonl".to_string(),
                "/tmp/a/child-2.jsonl".to_string(),
            ]
        );
    }

    #[test]
    fn cache_entry_validity_requires_key_and_timestamp_match() {
        let entry = FamilyIndexCacheEntry::<TestRow> {
            source_key: "/tmp/sessions".to_string(),
            updated_at: 42,
            index: FamilyIndex::build(Vec::new()),
        };

        assert!(entry.is_valid("/tmp/sessions", 42));
        assert!(!entry.is_valid("/tmp/other", 42));
        assert!(!entry.is_valid("/tmp/sessions", 43));
    }
}
