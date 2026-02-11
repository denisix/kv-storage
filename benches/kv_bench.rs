// Optimized benchmarks for KV Storage
// Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use kv_storage::storage::{DbWrapper, KeyMeta};
use kv_storage::util::hash::Hash;
use kv_storage::util::compression::Compressor;
use tempfile::TempDir;
use std::sync::Arc;
use std::time::Duration;

/// Benchmark write throughput - reuses DB for realistic performance
fn bench_write_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("write_throughput");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);

    let data_4k = vec![42u8; 4096];

    // Shared DB setup (created once, reused across samples)
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(DbWrapper::open(temp_dir.path().join("test_db")).unwrap());
    let keys_tree = db.keys_tree();
    let objects_tree = db.objects_tree();
    let refs_tree = db.refs_tree();

    for size in [100, 1_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut count = 0;
                for i in 0..size {
                    let key = format!("key_{:010}", i % 10000); // Avoid conflicts
                    let hash = Hash::compute(&data_4k);

                    objects_tree.insert(hash.as_ref(), data_4k.as_slice()).unwrap();

                    let meta = KeyMeta::new(hash, 4096);
                    let meta_bytes = bincode::serialize(&meta).unwrap();
                    keys_tree.insert(key.as_bytes(), meta_bytes).unwrap();

                    let mut ref_key = hash.as_ref().to_vec();
                    ref_key.extend_from_slice(key.as_bytes());
                    refs_tree.insert(&ref_key, b"1").unwrap();

                    count += 1;
                }
                black_box(count);
            });
        });
    }
    group.finish();
}

/// Benchmark read throughput - reads are fast
fn bench_read_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_throughput");
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(DbWrapper::open(temp_dir.path().join("test_db")).unwrap());
    let keys_tree = db.keys_tree();

    // Pre-populate with 10,000 keys
    let hash = Hash::compute(&vec![42u8; 4096]);
    let meta = KeyMeta::new(hash, 4096);
    let meta_bytes = bincode::serialize(&meta).unwrap();

    for i in 0..10_000 {
        let key = format!("key_{:010}", i);
        keys_tree.insert(key.as_bytes(), meta_bytes.clone()).unwrap();
    }

    for size in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut count = 0;
                for i in 0..size {
                    let key = format!("key_{:010}", i % 10_000);
                    if keys_tree.get(key.as_bytes()).unwrap().is_some() {
                        count += 1;
                    }
                }
                black_box(count);
            });
        });
    }
    group.finish();
}

/// Benchmark hash performance - pure CPU, very fast
fn bench_hash_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(100);

    let data_1k = vec![42u8; 1024];
    let data_4k = vec![42u8; 4096];
    let data_16k = vec![42u8; 16384];

    group.bench_function("xxhash3_128_1k", |b| {
        b.iter(|| black_box(Hash::compute(black_box(&data_1k))));
    });

    group.bench_function("xxhash3_128_4k", |b| {
        b.iter(|| black_box(Hash::compute(black_box(&data_4k))));
    });

    group.bench_function("xxhash3_128_16k", |b| {
        b.iter(|| black_box(Hash::compute(black_box(&data_16k))));
    });

    group.finish();
}

/// Benchmark compression with threshold
fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(100);

    let compressor = Compressor::new(3);

    let data_4k_repeatable = vec![42u8; 4096];
    let data_4k_random: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();

    group.bench_function("compress_4k_repeatable", |b| {
        b.iter(|| black_box(compressor.compress(black_box(&data_4k_repeatable)).unwrap()));
    });

    group.bench_function("compress_4k_random", |b| {
        b.iter(|| black_box(compressor.compress(black_box(&data_4k_random)).unwrap()));
    });

    let compressed_4k = compressor.compress(&data_4k_repeatable).unwrap();

    group.bench_function("decompress_4k", |b| {
        b.iter(|| black_box(compressor.decompress(black_box(&compressed_4k)).unwrap()));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_write_throughput,
    bench_read_throughput,
    bench_hash_compute,
    bench_compress,
);
criterion_main!(benches);
