use crate::error::Error;
use crate::storage::{StorageDb, KeyMeta};
use crate::util::hash::Hash;
use sled::{self, Transactional};

pub struct TransactionManager {
    db: StorageDb,
}

impl TransactionManager {
    pub fn new(db: StorageDb) -> Self {
        Self { db }
    }

    pub fn put_key_atomic(
        &self,
        key: &str,
        data: &[u8],
        hash: &Hash,
        size: u64,
    ) -> Result<bool, Error> {
        let db_ref = self.db.clone();
        let key_owned = key.to_string();
        let hash_owned = *hash;
        let data_owned = data.to_vec();

        // Get the trees before starting the transaction (now return &Tree directly)
        let keys_tree = db_ref.keys_tree();
        let objects_tree = db_ref.objects_tree();
        let refs_tree = db_ref.refs_tree();

        // Use sled's transaction API for atomic multi-tree operations
        let result = (keys_tree, objects_tree, refs_tree).transaction(|(keys_tree, objects_tree, refs_tree)| {
            // Check if key already exists - this is a conflict
            if keys_tree.get(key_owned.as_bytes())?.is_some() {
                return Err(sled::transaction::ConflictableTransactionError::Abort(
                    Error::Conflict(format!("Key '{}' already exists", key_owned))
                ));
            }

            // Check if object exists (for deduplication)
            let is_new_object = objects_tree.get(hash_owned.as_ref())?.is_none();

            if is_new_object {
                // Store the compressed object
                objects_tree.insert(hash_owned.as_ref(), data_owned.as_slice())?;
            }

            // Create key metadata
            let meta = KeyMeta::new(hash_owned, size);
            let meta_bytes = bincode::serialize(&meta)
                .map_err(|e| sled::transaction::ConflictableTransactionError::Abort(
                    Error::Storage(e.to_string())
                ))?;

            keys_tree.insert(key_owned.as_bytes(), meta_bytes)?;

            // Add ref entry
            let mut ref_key = hash_owned.as_ref().to_vec();
            ref_key.extend_from_slice(key_owned.as_bytes());
            refs_tree.insert(ref_key.as_slice(), b"1")?;

            Ok(is_new_object)
        });

        match result {
            Ok(is_new) => Ok(is_new),
            Err(sled::transaction::TransactionError::Abort(e)) => Err(e),
            Err(_) => {
                Err(Error::Conflict("Transaction conflict - please retry".to_string()))
            }
        }
    }

    pub fn delete_key_atomic(&self, key: &str) -> Result<Option<(Hash, u64)>, Error> {
        let db_ref = self.db.clone();
        let key_owned = key.to_string();

        // Get the trees (now return &Tree directly)
        let keys_tree = db_ref.keys_tree();
        let objects_tree = db_ref.objects_tree();
        let refs_tree = db_ref.refs_tree();

        // First, get the key metadata to find the hash
        let meta_bytes = keys_tree.get(key_owned.as_bytes())?
            .ok_or_else(|| Error::NotFound(format!("Key '{}' not found", key_owned)))?;

        let meta: KeyMeta = bincode::deserialize(&meta_bytes)?;
        let hash = meta.hash;
        let size = meta.size;

        // Check if we need to GC the object by counting refs
        let prefix = hash.as_ref();
        let _ref_count_before = refs_tree.scan_prefix(prefix).count();

        let result = (keys_tree, objects_tree, refs_tree).transaction(|(keys_tree, _objects_tree, refs_tree)| {
            // Get key metadata again to verify it still exists
            let meta_bytes = keys_tree.get(key_owned.as_bytes())?
                .ok_or_else(|| sled::transaction::ConflictableTransactionError::Abort(
                    Error::NotFound(format!("Key '{}' not found", key_owned))
                ))?;

            let meta: KeyMeta = bincode::deserialize(&meta_bytes)
                .map_err(|e| sled::transaction::ConflictableTransactionError::Abort(
                    Error::Storage(e.to_string())
                ))?;

            let hash = meta.hash;

            // Remove key
            keys_tree.remove(key_owned.as_bytes())?;

            // Remove ref
            let mut ref_key = hash.as_ref().to_vec();
            ref_key.extend_from_slice(key_owned.as_bytes());
            refs_tree.remove(ref_key.as_slice())?;

            Ok((hash, meta.size))
        });

        match result {
            Ok(_) => {
                // Check if object should be deleted (no more refs)
                // We do this outside the transaction since we can't iterate inside
                let ref_count_after = refs_tree.scan_prefix(prefix).count();
                if ref_count_after == 0 {
                    objects_tree.remove(hash.as_ref())?;
                }
                Ok(Some((hash, size)))
            }
            Err(sled::transaction::TransactionError::Abort(e)) => Err(e),
            Err(_) => {
                Err(Error::Conflict("Transaction conflict - please retry".to_string()))
            }
        }
    }

    pub fn update_key_atomic(
        &self,
        key: &str,
        data: &[u8],
        hash: &Hash,
        size: u64,
    ) -> Result<Option<Hash>, Error> {
        let db_ref = self.db.clone();
        let key_owned = key.to_string();
        let hash_owned = *hash;
        let data_owned = data.to_vec();

        // Get the trees (now return &Tree directly)
        let keys_tree = db_ref.keys_tree();
        let objects_tree = db_ref.objects_tree();
        let refs_tree = db_ref.refs_tree();

        let result = (keys_tree, objects_tree, refs_tree).transaction(|(keys_tree, objects_tree, refs_tree)| {
            // Get existing metadata
            let (old_hash, _should_gc_old) = if let Some(meta_bytes) = keys_tree.get(key_owned.as_bytes())? {
                let meta: KeyMeta = bincode::deserialize(&meta_bytes)
                    .map_err(|e| sled::transaction::ConflictableTransactionError::Abort(
                        Error::Storage(e.to_string())
                    ))?;
                let old_hash = meta.hash;

                // Remove old ref
                let mut old_ref_key = old_hash.as_ref().to_vec();
                old_ref_key.extend_from_slice(key_owned.as_bytes());
                refs_tree.remove(old_ref_key.as_slice())?;

                // For now, we don't GC in the transaction - we'll do it outside
                (Some(old_hash), false)
            } else {
                (None, false)
            };

            // Check if new object exists (for deduplication)
            let is_new_object = objects_tree.get(hash_owned.as_ref())?.is_none();

            if is_new_object {
                objects_tree.insert(hash_owned.as_ref(), data_owned.as_slice())?;
            }

            // Create new key metadata
            let meta = KeyMeta::new(hash_owned, size);
            let meta_bytes = bincode::serialize(&meta)
                .map_err(|e| sled::transaction::ConflictableTransactionError::Abort(
                    Error::Storage(e.to_string())
                ))?;

            keys_tree.insert(key_owned.as_bytes(), meta_bytes)?;

            // Add new ref
            let mut ref_key = hash_owned.as_ref().to_vec();
            ref_key.extend_from_slice(key_owned.as_bytes());
            refs_tree.insert(ref_key.as_slice(), b"1")?;

            Ok((old_hash, is_new_object))
        });

        match result {
            Ok((old_hash, _)) => Ok(old_hash),
            Err(sled::transaction::TransactionError::Abort(e)) => Err(e),
            Err(_) => {
                Err(Error::Conflict("Transaction conflict - please retry".to_string()))
            }
        }
    }

    pub fn batch_put(&self, operations: Vec<(String, Vec<u8>, Hash, u64)>) -> Result<Vec<Result<bool, Error>>, Error> {
        let mut results = Vec::new();

        for (key, data, hash, size) in operations {
            match self.put_key_atomic(&key, &data, &hash, size) {
                Ok(is_new) => results.push(Ok(is_new)),
                Err(e) => results.push(Err(e)),
            }
        }

        Ok(results)
    }
}
