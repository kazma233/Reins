//! Shared family-index engine for the session backends.
//!
//! Backends enumerate their own rows and group them into families — the
//! grouping strategies genuinely differ per source app (Claude groups by the
//! shared transcript sessionId, Codex walks the parent-thread chain, OpenCode
//! joins on SQL parent_id), so grouping stays out. Everything downstream of
//! grouping lives here exactly once: member and family ordering, and the
//! id-keyed family lookup that backs session addressing for every source.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

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
    /// id → family 下标,会话身份的唯一寻址面。双写胜者规则:family key
    /// (根自身 id)首见占位、成员 id 后写覆盖——与迁移前 id→path 双写图
    /// 逐字一致,由 family_index 前置核验测试钉住。
    sessions_by_id: HashMap<String, usize>,
}

impl<Row: FamilyRow + Clone> FamilyIndex<Row> {
    pub(crate) fn build(families: Vec<Family<Row>>) -> Self {
        let mut families = families;
        sort_members(&mut families);
        sort_families(&mut families);

        let mut sessions_by_id = HashMap::new();
        for (position, family) in families.iter().enumerate() {
            sessions_by_id
                .entry(family.root.family_root_id().to_string())
                .or_insert(position);
            for member in &family.members {
                sessions_by_id.insert(member.member_id().to_string(), position);
            }
        }

        Self {
            families,
            sessions_by_id,
        }
    }

    pub(crate) fn family_for_id(&self, source_session_id: &str) -> Option<Family<Row>> {
        let position = *self.sessions_by_id.get(source_session_id)?;
        Some(self.families[position].clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestRow {
        family_key: String,
        id: String,
        path: std::path::PathBuf,
        created_at: Option<i64>,
        updated_at: Option<i64>,
    }

    impl TestRow {
        fn new(id: &str, path: &str, created_at: i64, updated_at: i64) -> Self {
            Self {
                family_key: id.to_string(),
                id: id.to_string(),
                path: std::path::PathBuf::from(path),
                created_at: Some(created_at),
                updated_at: Some(updated_at),
            }
        }

        // family key 与成员 id 分离的行(claude 的 subagent 文件形态:
        // family_root_id 是共享 sessionId,member_id 是文件名派生的 agent id)。
        fn with_family_key(
            family_key: &str,
            id: &str,
            path: &str,
            created_at: i64,
            updated_at: i64,
        ) -> Self {
            Self {
                family_key: family_key.to_string(),
                id: id.to_string(),
                path: std::path::PathBuf::from(path),
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
            &self.family_key
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

    // —— id 唯一性核验:钉住双写胜者规则,id→family 直查必须保持同一胜者 ——

    // 同族内两条成员共用同一 id(claude 非 agent-* 文件回退 family key 的场景)。
    // 现状规则:成员排序(created_at, tie_breaker 升序)后逐条 insert,后插入者
    // 覆盖先插入者——created_at 更大的成员胜出。两条成员同属一个 family,
    // id 直查的家族归属无歧义。
    #[test]
    fn duplicate_member_id_within_family_has_single_family_owner() {
        let families = vec![Family {
            root: TestRow::new("shared", "/tmp/a/root.jsonl", 100, 500),
            members: vec![
                TestRow::new("shared", "/tmp/a/root.jsonl", 100, 500),
                TestRow::new("shared", "/tmp/a/unnamed.jsonl", 200, 600),
            ],
        }];

        let index = FamilyIndex::build(families);

        // 胜者是排序后靠后的成员(created_at=200 的 unnamed);两条同 id
        // 成员同族,id 直查的 family 归属无歧义。
        let family = index.family_for_id("shared").expect("shared resolves");
        assert_eq!(family.root.family_key, "shared");
        assert_eq!(
            family
                .members
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["shared", "shared"]
        );
    }

    // 跨族成员 id 碰撞:两个 family 各有一条 member_id 相同的成员。规则:
    // family 按(updated_at desc, root path asc)排序后依序插入,member id 的
    // insert 无条件覆盖——遍历中最后插入的 family(排序靠后、较旧者)胜出。
    #[test]
    fn cross_family_member_id_collision_last_inserted_family_wins() {
        let families = vec![
            Family {
                root: TestRow::new("root-new", "/tmp/new/root.jsonl", 100, 900),
                members: vec![
                    TestRow::new("root-new", "/tmp/new/root.jsonl", 100, 900),
                    TestRow::new("dup", "/tmp/new/dup.jsonl", 150, 950),
                ],
            },
            Family {
                root: TestRow::new("root-old", "/tmp/old/root.jsonl", 50, 500),
                members: vec![
                    TestRow::new("root-old", "/tmp/old/root.jsonl", 50, 500),
                    TestRow::new("dup", "/tmp/old/dup.jsonl", 60, 550),
                ],
            },
        ];

        let index = FamilyIndex::build(families);

        // 排序后 [root-new(updated 900), root-old(updated 500)];"dup" 的
        // 最后一次 insert 来自 root-old 族,后插入者胜出。
        let family = index.family_for_id("dup").expect("dup resolves");
        assert_eq!(family.root.id, "root-old");
        assert!(index.family_for_id("missing").is_none());
    }

    // 跨族 family key 碰撞(孤儿子会话各自成族的形态):family key 用
    // entry/or_insert 首见占位,排序靠前(updated_at 更大)的 family 胜出;
    // 成员 id 若与 family key 不同名,不会覆盖该占位。
    #[test]
    fn cross_family_key_collision_first_claimed_root_wins() {
        let families = vec![
            Family {
                root: TestRow::with_family_key(
                    "shared-key",
                    "member-new",
                    "/tmp/new/root.jsonl",
                    100,
                    900,
                ),
                members: vec![TestRow::with_family_key(
                    "shared-key",
                    "member-new",
                    "/tmp/new/root.jsonl",
                    100,
                    900,
                )],
            },
            Family {
                root: TestRow::with_family_key(
                    "shared-key",
                    "member-old",
                    "/tmp/old/root.jsonl",
                    50,
                    500,
                ),
                members: vec![TestRow::with_family_key(
                    "shared-key",
                    "member-old",
                    "/tmp/old/root.jsonl",
                    50,
                    500,
                )],
            },
        ];

        let index = FamilyIndex::build(families);

        // 首见占位:排序靠前的 root-new 族先 claim "shared-key"。
        let family = index
            .family_for_id("shared-key")
            .expect("family key resolves");
        assert_eq!(family.root.id, "member-new");

        // 各自的成员 id 独立解析,不受 family key 占位影响。
        assert_eq!(
            index
                .family_for_id("member-old")
                .expect("member-old resolves")
                .root
                .id,
            "member-old"
        );
    }

    #[test]
    fn family_key_resolves_root_and_member_id_resolves_member() {
        let index = FamilyIndex::build(sample_families());

        assert_eq!(
            index.family_for_id("root-a").expect("root-a").root.id,
            "root-a"
        );
        let child = index.family_for_id("child-a1").expect("child-a1");
        assert_eq!(child.root.id, "root-a");
        assert!(index.family_for_id("missing").is_none());
    }

    #[test]
    fn index_is_deterministic_across_input_orders() {
        let mut families = sample_families();
        families[0].members.reverse();

        let index = FamilyIndex::build(sample_families());
        let shuffled = FamilyIndex::build(families);

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
}
