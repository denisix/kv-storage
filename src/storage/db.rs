use crate::error::Error;
use sled::{Db as SledDb, Tree, IVec, Mode};
use std::sync::Arc;
use std::path::Path;

const KEYS_TREE: &str = "keys";
const OBJECTS_TREE: &str = "objects";
const REFS_TREE: &str = "refs";

const DEFAULT_CACHE_CAPACITY: usize = 1_024_000_000; // 1GB

#[derive(Clone)]
pub struct DbWrapper {
    db: Arc<SledDb>,
    // Cached tree handles for better performance
    keys_tree: Arc<Tree>,
    objects_tree: Arc<Tree>,
    refs_tree: Arc<Tree>,
}

impl DbWrapper {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Self::open_with_config(path, None, None)
    }

    pub fn open_with_config<P: AsRef<Path>>(
        path: P,
        cache_capacity_bytes: Option<usize>,
        flush_interval_ms: Option<u64>,
    ) -> Result<Self, Error> {
        let cache_capacity = cache_capacity_bytes.unwrap_or(DEFAULT_CACHE_CAPACITY) as u64;

        // Optimized sled configuration for maximum write throughput
        // Mode::HighThroughput = faster writes, larger file size
        // Mode::LowSpace = smaller file size, slightly slower writes
        let config = sled::Config::default()
            .path(path)
            .cache_capacity(cache_capacity)
            .flush_every_ms(flush_interval_ms)
            .mode(Mode::HighThroughput);  // Optimized for write speed

        // Open and cache tree handles to avoid repeated lookups
        let db = Arc::new(config.open()?);
        let keys_tree = Arc::new(db.open_tree(KEYS_TREE)?);
        let objects_tree = Arc::new(db.open_tree(OBJECTS_TREE)?);
        let refs_tree = Arc::new(db.open_tree(REFS_TREE)?);

        Ok(Self {
            db,
            keys_tree,
            objects_tree,
            refs_tree,
        })
    }

    #[inline]
    pub fn keys_tree(&self) -> &Tree {
        &self.keys_tree
    }

    #[inline]
    pub fn objects_tree(&self) -> &Tree {
        &self.objects_tree
    }

    #[inline]
    pub fn refs_tree(&self) -> &Tree {
        &self.refs_tree
    }

    #[inline]
    pub fn inner(&self) -> &SledDb {
        &self.db
    }

    pub fn flush(&self) -> Result<(), Error> {
        self.db.flush()?;
        Ok(())
    }

    pub fn count_tree(&self, tree_name: &str) -> Result<usize, Error> {
        // Use cached tree handles for known trees to avoid expensive lookups
        let count = match tree_name {
            KEYS_TREE => self.keys_tree.iter().count(),
            OBJECTS_TREE => self.objects_tree.iter().count(),
            REFS_TREE => self.refs_tree.iter().count(),
            _ => self.db.open_tree(tree_name)?.iter().count(),
        };
        Ok(count)
    }

    pub fn list_tree_paginated(&self, tree_name: &str, offset: usize, limit: usize) -> Result<Vec<(Vec<u8>, IVec)>, Error> {
        // Use cached tree handles for known trees to avoid expensive lookups
        let result: Vec<(Vec<u8>, IVec)> = match tree_name {
            KEYS_TREE => {
                self.keys_tree.iter()
                    .skip(offset)
                    .take(limit)
                    .map(|item| item.map(|(k, v)| (k.to_vec(), v)))
                    .collect::<Result<_, _>>()?
            }
            OBJECTS_TREE => {
                self.objects_tree.iter()
                    .skip(offset)
                    .take(limit)
                    .map(|item| item.map(|(k, v)| (k.to_vec(), v)))
                    .collect::<Result<_, _>>()?
            }
            REFS_TREE => {
                self.refs_tree.iter()
                    .skip(offset)
                    .take(limit)
                    .map(|item| item.map(|(k, v)| (k.to_vec(), v)))
                    .collect::<Result<_, _>>()?
            }
            _ => {
                self.db.open_tree(tree_name)?.iter()
                    .skip(offset)
                    .take(limit)
                    .map(|item| item.map(|(k, v)| (k.to_vec(), v)))
                    .collect::<Result<_, _>>()?
            }
        };
        Ok(result)
    }

    pub fn count_keys(&self) -> usize {
        self.keys_tree.len()
    }

    pub fn count_objects(&self) -> usize {
        self.objects_tree.len()
    }

    pub fn count_refs(&self) -> usize {
        self.refs_tree.len()
    }
}

pub type StorageDb = Arc<DbWrapper>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_methods() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = DbWrapper::open(temp_dir.path().join("test")).unwrap();

        // All counts should be 0 initially
        assert_eq!(db.count_keys(), 0);
        assert_eq!(db.count_objects(), 0);
        assert_eq!(db.count_refs(), 0);
    }

    #[test]
    fn test_db_reopen() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test");

        // Create and populate
        {
            let db = DbWrapper::open(&path).unwrap();
            let keys_tree = db.keys_tree();
            keys_tree.insert(b"key1", b"value1").unwrap();
        }

        // Reopen and verify
        let db = DbWrapper::open(&path).unwrap();
        let keys_tree = db.keys_tree();
        let result = keys_tree.get(b"key1").unwrap();
        assert_eq!(result.unwrap(), b"value1");
    }

    #[test]
    fn test_list_tree_paginated_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = DbWrapper::open(temp_dir.path().join("test")).unwrap();

        let result = db.list_tree_paginated("keys", 0, 10).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_list_tree_paginated_with_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = DbWrapper::open(temp_dir.path().join("test")).unwrap();
        let keys_tree = db.keys_tree();

        for i in 0..5 {
            keys_tree.insert(format!("key{}", i).as_bytes(), format!("value{}", i).as_bytes()).unwrap();
        }

        let result = db.list_tree_paginated("keys", 0, 2).unwrap();
        assert_eq!(result.len(), 2);
    }
}
