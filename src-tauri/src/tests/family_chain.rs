// 父链分组的策略测试:两种孤儿归属(opencode/zcode 认领缺失父 id、
// codex 保留最后已知 id)与环检测是收编进 family_index 的共享行为,这里
// 按策略逐一钉住;与各来源迁移前的逐字等价由既有行为测试守卫。

use std::collections::HashMap;

use crate::session::family_index::{
    OrphanPolicy, ParentChainRow, group_families_by_parent_chain, member_root,
    root_id_by_parent_chain,
};

#[derive(Clone, Debug)]
struct Row {
    id: &'static str,
    parent: Option<&'static str>,
}

impl ParentChainRow for Row {
    fn chain_id(&self) -> &str {
        self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent
    }
}

fn by_id(rows: &[Row]) -> HashMap<String, Row> {
    rows.iter()
        .cloned()
        .map(|row| (row.id.to_string(), row))
        .collect()
}

#[test]
fn claim_missing_parent_orphans_form_own_family() {
    let rows = [
        Row {
            id: "root",
            parent: None,
        },
        Row {
            id: "child",
            parent: Some("ghost"),
        },
    ];
    let map = by_id(&rows);
    assert_eq!(
        root_id_by_parent_chain(&rows[1], &map, OrphanPolicy::ClaimMissingParent),
        "ghost"
    );
    assert_eq!(
        root_id_by_parent_chain(&rows[0], &map, OrphanPolicy::ClaimMissingParent),
        "root"
    );

    let families = group_families_by_parent_chain(
        rows.to_vec(),
        OrphanPolicy::ClaimMissingParent,
        member_root,
    );
    assert_eq!(families.len(), 2);
    // 认领的族里没有该 id 的行,root 回退首成员。
    let orphan = families
        .iter()
        .find(|family| family.root.chain_id() == "child")
        .expect("orphan family");
    assert_eq!(orphan.members.len(), 1);
}

#[test]
fn keep_last_known_attaches_orphan_to_surviving_ancestor() {
    // root 缺席:mid 的链断在父上,leaf 经 mid 归到 mid——孤儿挂靠最近的
    // 存活祖先而不是认领缺失 id。
    let rows = [
        Row {
            id: "mid",
            parent: Some("root"),
        },
        Row {
            id: "leaf",
            parent: Some("mid"),
        },
    ];
    let map = by_id(&rows);
    assert_eq!(
        root_id_by_parent_chain(&rows[0], &map, OrphanPolicy::KeepLastKnown),
        "mid"
    );
    assert_eq!(
        root_id_by_parent_chain(&rows[1], &map, OrphanPolicy::KeepLastKnown),
        "mid"
    );
}

#[test]
fn cycle_stops_at_visited_node() {
    let rows = [
        Row {
            id: "a",
            parent: Some("a"),
        },
        Row {
            id: "b",
            parent: None,
        },
    ];
    let map = by_id(&rows);
    assert_eq!(
        root_id_by_parent_chain(&rows[0], &map, OrphanPolicy::ClaimMissingParent),
        "a"
    );
    assert_eq!(
        root_id_by_parent_chain(&rows[0], &map, OrphanPolicy::KeepLastKnown),
        "a"
    );
}

#[test]
fn member_root_prefers_root_id_row_then_first() {
    let rows = [
        Row {
            id: "second",
            parent: Some("root"),
        },
        Row {
            id: "root",
            parent: None,
        },
    ];
    assert_eq!(member_root("root", &rows).unwrap().chain_id(), "root");
    // 认领的族根 id 无对应行时回退首成员。
    assert_eq!(member_root("ghost", &rows).unwrap().chain_id(), "second");
}
