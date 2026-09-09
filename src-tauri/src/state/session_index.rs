use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};

use crate::session::model::{SessionFileEntry, SourceApp};

#[derive(Clone, Debug, Default)]
pub(crate) struct SessionFileCatalog {
    pub(crate) entries: Arc<[SessionFileEntry]>,
}

#[derive(Clone, Default)]
pub(crate) struct SessionIndexState {
    catalogs: Arc<Mutex<HashMap<SourceApp, SessionFileCatalog>>>,
}

impl SessionIndexState {
    pub(crate) fn catalog(&self, source_app: SourceApp) -> Result<Option<SessionFileCatalog>> {
        Ok(self
            .catalogs
            .lock()
            .map_err(|_| anyhow!("Session index cache lock was poisoned"))?
            .get(&source_app)
            .cloned())
    }

    pub(crate) fn store_catalog(
        &self,
        source_app: SourceApp,
        catalog: SessionFileCatalog,
    ) -> Result<()> {
        self.catalogs
            .lock()
            .map_err(|_| anyhow!("Session index cache lock was poisoned"))?
            .insert(source_app, catalog);
        Ok(())
    }

    pub(crate) fn clear(&self) -> Result<()> {
        self.catalogs
            .lock()
            .map_err(|_| anyhow!("Session index cache lock was poisoned"))?
            .clear();
        Ok(())
    }
}
