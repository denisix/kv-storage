use bytes::Bytes;
use serde::{Deserialize, Serialize};

pub type XxHash128 = [u8; 16];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hash(pub XxHash128);

impl AsRef<[u8]> for Hash {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Hash {
    /// Compute xxHash3-128 - fast non-cryptographic 128-bit hash
    #[inline]
    pub fn compute(data: &[u8]) -> Self {
        use twox_hash::XxHash3_128;
        let mut hasher = XxHash3_128::default();
        hasher.write(data);
        let result = hasher.finish_128();
        Hash(result.to_be_bytes())
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    pub fn to_hex_string(&self) -> String {
        hex::encode(self.0)
    }

    #[inline]
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

pub fn hash_bytes(data: &[u8]) -> Hash {
    Hash::compute(data)
}

pub fn hash_bytes_stream(data: &Bytes) -> Hash {
    Hash::compute(data.as_ref())
}

pub fn hash_to_string(hash: &Hash) -> String {
    hash.to_hex_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xxhash3_128_hash() {
        let data = b"hello world";
        let hash = Hash::compute(data);
        assert_eq!(hash.as_bytes().len(), 16);
    }

    #[test]
    fn test_hash_consistency() {
        let data = b"test data";
        let hash1 = Hash::compute(data);
        let hash2 = Hash::compute(data);
        assert_eq!(hash1.0, hash2.0);
    }

    #[test]
    fn test_hash_different_input() {
        let data1 = b"test data";
        let data2 = b"different data";
        let hash1 = Hash::compute(data1);
        let hash2 = Hash::compute(data2);
        assert_ne!(hash1.0, hash2.0);
    }

    #[test]
    fn test_hash_to_hex() {
        let data = b"test";
        let hash = Hash::compute(data);
        let hex = hash.to_hex_string();
        assert_eq!(hex.len(), 32); // 16 bytes = 32 hex chars
    }
}
