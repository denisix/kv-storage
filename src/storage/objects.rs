use crate::error::Error;
use crate::util::compression::Compressor;
use crate::util::hash::Hash;
use std::sync::Arc;

pub struct ObjectStore {
    tree: sled::Tree,
    refs_tree: sled::Tree,
    compressor: Arc<Compressor>,
}

impl ObjectStore {
    pub fn new(tree: &sled::Tree, refs_tree: &sled::Tree, compressor: Arc<Compressor>) -> Self {
        Self {
            tree: tree.clone(),
            refs_tree: refs_tree.clone(),
            compressor,
        }
    }

    pub fn put(&self, hash: &Hash, data: &[u8], key: &str) -> Result<bool, Error> {
        // Check if object already exists (deduplication check)
        let exists = self.tree.contains_key(hash)?;

        if !exists {
            // Compress and store
            let compressed = self.compressor.compress(data)?;
            self.tree.insert(hash, compressed)?;
        }

        // Add to refs tree (reverse lookup: hash -> set of keys)
        self.add_ref(hash, key)?;

        Ok(!exists) // Return true if this was a new object
    }

    pub fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>, Error> {
        if let Some(compressed) = self.tree.get(hash)? {
            let data = self.compressor.decompress(&compressed)?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    pub fn exists(&self, hash: &Hash) -> Result<bool, Error> {
        Ok(self.tree.contains_key(hash)?)
    }

    pub fn delete(&self, hash: &Hash) -> Result<(), Error> {
        self.tree.remove(hash)?;
        Ok(())
    }

    pub fn remove_ref(&self, hash: &Hash, key: &str) -> Result<bool, Error> {
        let ref_key = self.make_ref_key(hash, key);
        self.refs_tree.remove(&ref_key)?;

        // Check if any refs remain for this hash
        let prefix = hash.to_vec();
        let has_refs = self.refs_tree.scan_prefix(&prefix).next().is_some();

        if !has_refs {
            self.delete(hash)?;
            Ok(true) // Object was deleted (no more refs)
        } else {
            Ok(false) // Object still has refs
        }
    }

    pub fn get_ref_count(&self, hash: &Hash) -> Result<usize, Error> {
        let prefix = hash.to_vec();
        Ok(self.refs_tree.scan_prefix(&prefix).count())
    }

    pub fn count(&self) -> Result<usize, Error> {
        Ok(self.tree.len())
    }

    pub fn total_size(&self) -> Result<u64, Error> {
        let mut total = 0u64;
        for item in self.tree.iter() {
            let (_, value) = item?;
            total += value.len() as u64;
        }
        Ok(total)
    }

    fn add_ref(&self, hash: &Hash, key: &str) -> Result<(), Error> {
        let ref_key = self.make_ref_key(hash, key);
        self.refs_tree.insert(&ref_key, b"1")?;
        Ok(())
    }

    fn make_ref_key(&self, hash: &Hash, key: &str) -> Vec<u8> {
        let mut ref_key = hash.to_vec();
        ref_key.extend_from_slice(key.as_bytes());
        ref_key
    }
}
