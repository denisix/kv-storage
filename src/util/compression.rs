use crate::error::Error;
use zstd::stream::{encode_all, decode_all};

pub struct Compressor {
    level: i32,
    min_compress_size: usize,
}

impl Compressor {
    pub fn new(level: i32) -> Self {
        Self {
            level: level.max(1).min(3), // Clamp to 1-3 for speed/compression balance
            min_compress_size: 512, // Compress data >= 512 bytes
        }
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        // Skip compression for very small data (overhead not worth it)
        if data.len() < self.min_compress_size {
            return Ok(data.to_vec());
        }

        encode_all(data, self.level)
            .map_err(|e| Error::Compression(format!("Compression failed: {}", e)))
    }

    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, Error> {
        // Quick check: if data doesn't look like zstd, return as-is
        if data.len() < 4 {
            return Ok(data.to_vec());
        }

        // Check for zstd magic number to avoid unnecessary decompression attempts
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if magic != 0xFD2FB528 {
            return Ok(data.to_vec());
        }

        // Try to decompress - if it fails, return as-is (handles uncompressed data)
        match decode_all(data) {
            Ok(result) => Ok(result),
            Err(_) => Ok(data.to_vec()),
        }
    }

    #[inline]
    pub fn should_compress(&self, size: usize) -> bool {
        size >= self.min_compress_size
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let compressor = Compressor::new(1);
        // Use more repetitive data that will compress better
        let original = b"hello world, this is a test data for compression. ".repeat(100);

        let compressed = compressor.compress(&original).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(original, decompressed.as_slice());
        assert!(compressed.len() < original.len()); // Should compress
    }

    #[test]
    fn test_compress_small_data() {
        let compressor = Compressor::new(1);
        let original = b"hi";

        let compressed = compressor.compress(original).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(original.to_vec(), decompressed);
        // Small data should not be compressed (returned as-is)
        assert_eq!(original.to_vec(), compressed);
    }

    #[test]
    fn test_compress_threshold() {
        let compressor = Compressor::new(1);

        // Small data (< 1KB) should not be compressed
        let small_data = b"x".repeat(100);
        let compressed = compressor.compress(&small_data).unwrap();
        assert_eq!(small_data.len(), compressed.len()); // Same size = not compressed

        // Large data (> 1KB) should be compressed
        let large_data = b"hello world, this is a test. ".repeat(100);
        let compressed = compressor.compress(&large_data).unwrap();
        assert!(compressed.len() < large_data.len()); // Smaller = compressed
    }

    #[test]
    fn test_decompress_uncompressed() {
        let compressor = Compressor::new(1);
        let data = b"test data under 1KB";

        // Data stored uncompressed should be returned as-is
        let result = compressor.decompress(data).unwrap();
        assert_eq!(data.to_vec(), result);
    }
}
