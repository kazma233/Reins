use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};

use crate::workspace::types::SkillDiscoverySnapshot;

// Discovery snapshots are only consumed while their dialog is open; ids are
// never reused after a re-discover. Bounding the map keeps repeated "discover"
// clicks from accumulating snapshots forever.
const MAX_SNAPSHOTS: usize = 4;

#[derive(Default)]
struct SnapshotStore {
    entries: HashMap<String, Arc<SkillDiscoverySnapshot>>,
    insertion_order: Vec<String>,
}

impl SnapshotStore {
    fn insert(&mut self, discovery_id: String, snapshot: Arc<SkillDiscoverySnapshot>) {
        if !self.entries.contains_key(&discovery_id) {
            self.insertion_order.push(discovery_id.clone());
        }
        self.entries.insert(discovery_id, snapshot);

        while self.insertion_order.len() > MAX_SNAPSHOTS {
            let evicted = self.insertion_order.remove(0);
            self.entries.remove(&evicted);
        }
    }

    fn remove(&mut self, discovery_id: &str) {
        self.entries.remove(discovery_id);
        self.insertion_order.retain(|id| id != discovery_id);
    }
}

#[derive(Clone, Default)]
pub(crate) struct SkillDiscoveryState {
    snapshots: Arc<Mutex<SnapshotStore>>,
}

impl SkillDiscoveryState {
    pub(crate) fn load(&self, discovery_id: &str) -> Result<Option<Arc<SkillDiscoverySnapshot>>> {
        Ok(self
            .snapshots
            .lock()
            .map_err(|_| anyhow!("Skill discovery cache lock was poisoned"))?
            .entries
            .get(discovery_id)
            .cloned())
    }

    pub(crate) fn store(
        &self,
        discovery_id: String,
        snapshot: SkillDiscoverySnapshot,
    ) -> Result<()> {
        self.snapshots
            .lock()
            .map_err(|_| anyhow!("Skill discovery cache lock was poisoned"))?
            .insert(discovery_id, Arc::new(snapshot));
        Ok(())
    }

    pub(crate) fn remove(&self, discovery_id: &str) -> Result<()> {
        self.snapshots
            .lock()
            .map_err(|_| anyhow!("Skill discovery cache lock was poisoned"))?
            .remove(discovery_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SkillDiscoverySnapshot {
        SkillDiscoverySnapshot {
            source: crate::workspace::types::DiscoverySourceDefinition::Local {
                root_path: std::path::PathBuf::from("/tmp"),
            },
            skills: Arc::from(Vec::new()),
        }
    }

    #[test]
    fn evicts_oldest_snapshot_beyond_capacity() {
        let state = SkillDiscoveryState::default();
        for index in 0..(MAX_SNAPSHOTS + 2) {
            state.store(format!("id-{index}"), snapshot()).unwrap();
        }

        assert!(state.load("id-0").unwrap().is_none());
        assert!(state.load("id-1").unwrap().is_none());
        assert!(
            state
                .load(&format!("id-{MAX_SNAPSHOTS}"))
                .unwrap()
                .is_some()
        );
        assert!(
            state
                .load(&format!("id-{}", MAX_SNAPSHOTS + 1))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn remove_allows_storing_more_without_eviction() {
        let state = SkillDiscoveryState::default();
        for index in 0..MAX_SNAPSHOTS {
            state.store(format!("id-{index}"), snapshot()).unwrap();
        }
        state.remove("id-0").unwrap();
        state.store("id-new".to_string(), snapshot()).unwrap();

        assert!(state.load("id-1").unwrap().is_some());
        assert!(state.load("id-new").unwrap().is_some());
    }
}
