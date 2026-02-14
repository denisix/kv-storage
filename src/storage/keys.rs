use crate::error::Error;
use crate::util::hash::Hash;
use serde::{Deserialize, Serialize};
use sled::IVec;
use std::ops::Deref;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMeta {
    pub hash: Hash,
    pub size: u64,
    pub refs: u64,
    pub created_at: u64,
}

impl KeyMeta {
    pub fn new(hash: Hash, size: u64) -> Self {
        Self {
            hash,
            size,
            refs: 1,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn increment_ref(&mut self) {
        self.refs += 1;
    }

    pub fn decrement_ref(&mut self) -> Result<(), Error> {
        if self.refs == 0 {
            return Err(Error::Internal("Reference count underflow".to_string()));
        }
        self.refs -= 1;
        Ok(())
    }
}

pub struct KeyStore {
    tree: sled::Tree,
}

impl KeyStore {
    pub fn new(tree: &sled::Tree) -> Self {
        Self { tree: tree.clone() }
    }

    pub fn get(&self, key: &str) -> Result<Option<KeyMeta>, Error> {
        self.tree.get(key.as_bytes())?
            .map(|ivec| self.deserialize_meta(&ivec))
            .transpose()
    }

    pub fn set(&self, key: &str, meta: &KeyMeta) -> Result<(), Error> {
        let data = bincode::serialize(meta)?;
        self.tree.insert(key.as_bytes(), data)?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), Error> {
        self.tree.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn exists(&self, key: &str) -> Result<bool, Error> {
        Ok(self.tree.contains_key(key.as_bytes())?)
    }

    pub fn list(&self) -> Result<Vec<(String, KeyMeta)>, Error> {
        let mut result = Vec::new();
        for item in self.tree.iter() {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            let meta = self.deserialize_meta(&value)?;
            result.push((key_str, meta));
        }
        Ok(result)
    }

    pub fn list_paginated(&self, offset: usize, limit: usize) -> Result<Vec<(String, KeyMeta)>, Error> {
        let mut result = Vec::new();
        for (idx, item) in self.tree.iter().enumerate() {
            if idx < offset {
                continue;
            }
            if result.len() >= limit {
                break;
            }
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            let meta = self.deserialize_meta(&value)?;
            result.push((key_str, meta));
        }
        Ok(result)
    }

    pub fn count(&self) -> Result<usize, Error> {
        Ok(self.tree.len())
    }

    fn deserialize_meta(&self, ivec: &IVec) -> Result<KeyMeta, Error> {
        bincode::deserialize(ivec.deref()).map_err(Into::into)
    }

    pub fn update_ref_count(&self, key: &str, delta: i32) -> Result<Option<KeyMeta>, Error> {
        if let Some(mut meta) = self.get(key)? {
            if delta > 0 {
                meta.refs = meta.refs.saturating_add(delta as u64);
            } else {
                let new_refs = meta.refs.saturating_sub((-delta) as u64);
                meta.refs = new_refs;
            }
            self.set(key, &meta)?;
            Ok(Some(meta))
        } else {
            Ok(None)
        }
    }
}
