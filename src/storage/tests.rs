#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::util::compression::Compressor;
    use crate::util::hash::Hash;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, StorageDb) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");
        let db = Arc::new(DbWrapper::open(&db_path).unwrap());
        (temp_dir, db)
    }

    #[test]
    fn test_db_open_creates_trees() {
        let (_temp, db) = setup_test_db();
        // Verify trees are accessible
        assert!(db.keys_tree().is_empty());
        assert!(db.objects_tree().is_empty());
        assert!(db.refs_tree().is_empty());
    }

    #[test]
    fn test_key_store_set_and_get() {
        let (_temp, db) = setup_test_db();
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);

        let hash = Hash([1u8; 16]); // xxHash3-128 is 16 bytes
        let meta = KeyMeta::new(hash, 100);

        key_store.set("test_key", &meta).unwrap();

        let retrieved = key_store.get("test_key").unwrap();
        assert!(retrieved.is_some());
        let retrieved_meta = retrieved.unwrap();
        assert_eq!(retrieved_meta.hash, hash);
        assert_eq!(retrieved_meta.size, 100);
    }

    #[test]
    fn test_key_store_delete() {
        let (_temp, db) = setup_test_db();
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);

        let hash = Hash([1u8; 16]);
        let meta = KeyMeta::new(hash, 100);
        key_store.set("test_key", &meta).unwrap();

        assert!(key_store.exists("test_key").unwrap());
        key_store.delete("test_key").unwrap();
        assert!(!key_store.exists("test_key").unwrap());
    }

    #[test]
    fn test_key_store_list() {
        let (_temp, db) = setup_test_db();
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);

        // Add multiple keys
        for i in 0..5 {
            let hash = Hash([i as u8; 16]);
            let meta = KeyMeta::new(hash, i * 10);
            key_store.set(&format!("key_{}", i), &meta).unwrap();
        }

        let all_keys = key_store.list().unwrap();
        assert_eq!(all_keys.len(), 5);
    }

    #[test]
    fn test_key_store_list_paginated() {
        let (_temp, db) = setup_test_db();
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);

        // Add multiple keys
        for i in 0..10 {
            let hash = Hash([i as u8; 16]);
            let meta = KeyMeta::new(hash, i * 10);
            key_store.set(&format!("key_{:02}", i), &meta).unwrap();
        }

        let page1 = key_store.list_paginated(0, 3).unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = key_store.list_paginated(3, 3).unwrap();
        assert_eq!(page2.len(), 3);
    }

    #[test]
    fn test_key_meta_ref_counting() {
        let hash = Hash([1u8; 16]);
        let mut meta = KeyMeta::new(hash, 100);
        assert_eq!(meta.refs, 1);

        meta.increment_ref();
        assert_eq!(meta.refs, 2);

        meta.decrement_ref().unwrap();
        assert_eq!(meta.refs, 1);
    }

    #[test]
    fn test_key_meta_ref_count_underflow() {
        let hash = Hash([1u8; 16]);
        let mut meta = KeyMeta::new(hash, 100);
        meta.refs = 0;

        let result = meta.decrement_ref();
        assert!(result.is_err());
    }

    #[test]
    fn test_object_store_compress_and_decompress() {
        let (_temp, db) = setup_test_db();
        let compressor = Arc::new(Compressor::new(1));
        let objects_tree = db.objects_tree();
        let refs_tree = db.refs_tree();
        let object_store = ObjectStore::new(objects_tree, refs_tree, compressor);

        let data = b"test data for compression";
        let hash = Hash([1u8; 16]);

        let is_new = object_store.put(&hash, data, "test_key").unwrap();
        assert!(is_new); // First time should be new

        // Check if it exists
        assert!(object_store.exists(&hash).unwrap());

        // Get and verify
        let retrieved = object_store.get(&hash).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_object_store_deduplication() {
        let (_temp, db) = setup_test_db();
        let compressor = Arc::new(Compressor::new(1));
        let objects_tree = db.objects_tree();
        let refs_tree = db.refs_tree();
        let object_store = ObjectStore::new(objects_tree, refs_tree, compressor);

        let data = b"test data";
        let hash = Hash([1u8; 16]);

        // First put
        let is_new1 = object_store.put(&hash, data, "key1").unwrap();
        assert!(is_new1);

        // Second put with same data but different key
        let is_new2 = object_store.put(&hash, data, "key2").unwrap();
        assert!(!is_new2); // Should not be new (deduplicated)

        // Both refs should exist
        assert_eq!(object_store.get_ref_count(&hash).unwrap(), 2);
    }

    #[test]
    fn test_object_store_remove_ref() {
        let (_temp, db) = setup_test_db();
        let compressor = Arc::new(Compressor::new(1));
        let objects_tree = db.objects_tree();
        let refs_tree = db.refs_tree();
        let object_store = ObjectStore::new(objects_tree, refs_tree, compressor);

        let data = b"test data";
        let hash = Hash([1u8; 16]);

        // Add two refs
        object_store.put(&hash, data, "key1").unwrap();
        object_store.put(&hash, data, "key2").unwrap();

        // Remove one ref
        let deleted = object_store.remove_ref(&hash, "key1").unwrap();
        assert!(!deleted); // Should not be deleted yet

        // Remove second ref
        let deleted = object_store.remove_ref(&hash, "key2").unwrap();
        assert!(deleted); // Should be deleted now

        // Object should be gone
        assert!(!object_store.exists(&hash).unwrap());
    }

    #[test]
    fn test_transaction_put_atomic() {
        let (_temp, db) = setup_test_db();
        let tx_manager = TransactionManager::new(db.clone());

        let data = b"atomic put data";
        let compressed = zstd::stream::encode_all(&data[..], 1).unwrap();
        let hash = Hash([1u8; 16]);
        let size = data.len() as u64;

        // Atomic put
        let is_new = tx_manager.put_key_atomic("test_key", &compressed, &hash, size).unwrap();
        assert!(is_new);

        // Verify key exists
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);
        let meta = key_store.get("test_key").unwrap();
        assert!(meta.is_some());
    }

    #[test]
    fn test_transaction_put_conflict() {
        let (_temp, db) = setup_test_db();
        let tx_manager = TransactionManager::new(db.clone());

        let data = b"atomic put data";
        let compressed = zstd::stream::encode_all(&data[..], 1).unwrap();
        let hash = Hash([1u8; 16]);
        let size = data.len() as u64;

        // First put
        tx_manager.put_key_atomic("test_key", &compressed, &hash, size).unwrap();

        // Second put with same key should fail
        let result = tx_manager.put_key_atomic("test_key", &compressed, &hash, size);
        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_delete_atomic() {
        let (_temp, db) = setup_test_db();
        let tx_manager = TransactionManager::new(db.clone());

        let data = b"delete test data";
        let compressed = zstd::stream::encode_all(&data[..], 1).unwrap();
        let hash = Hash([1u8; 16]);
        let size = data.len() as u64;

        // Put key first
        tx_manager.put_key_atomic("test_key", &compressed, &hash, size).unwrap();

        // Delete atomically
        let result = tx_manager.delete_key_atomic("test_key").unwrap();
        assert!(result.is_some());

        // Verify key is gone
        let keys_tree = db.keys_tree();
        let key_store = KeyStore::new(keys_tree);
        assert!(key_store.get("test_key").unwrap().is_none());
    }

    #[test]
    fn test_transaction_delete_nonexistent() {
        let (_temp, db) = setup_test_db();
        let tx_manager = TransactionManager::new(db);

        let result = tx_manager.delete_key_atomic("nonexistent_key");
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_ratio() {
        let compressor = Compressor::new(1);

        // Highly compressible data
        let data = vec![42u8; 10000];
        let compressed = compressor.compress(&data).unwrap();
        assert!(compressed.len() < data.len());

        // Verify decompression
        let decompressed = compressor.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compression_levels() {
        let data = b"test data for compression level test";

        // Test different levels
        for level in [1, 3, 10].iter() {
            let compressor = Compressor::new(*level);
            let compressed = compressor.compress(data).unwrap();
            let decompressed = compressor.decompress(&compressed).unwrap();
            assert_eq!(decompressed.as_slice(), data);
        }
    }
}
