use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct Metrics {
    pub keys_total: AtomicU64,
    pub objects_total: AtomicU64,
    pub bytes_total: AtomicU64,
    pub puts_total: AtomicU64,
    pub gets_total: AtomicU64,
    pub deletes_total: AtomicU64,
    pub dedup_hits: AtomicU64,
}

impl Metrics {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys_total: AtomicU64::new(0),
            objects_total: AtomicU64::new(0),
            bytes_total: AtomicU64::new(0),
            puts_total: AtomicU64::new(0),
            gets_total: AtomicU64::new(0),
            deletes_total: AtomicU64::new(0),
            dedup_hits: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn inc_puts(&self) {
        self.puts_total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_gets(&self) {
        self.gets_total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_deletes(&self) {
        self.deletes_total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_dedup_hits(&self) {
        self.dedup_hits.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn set_keys(&self, count: u64) {
        self.keys_total.store(count, Ordering::Relaxed);
    }

    #[inline]
    pub fn set_objects(&self, count: u64) {
        self.objects_total.store(count, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_bytes(&self, bytes: u64) {
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Saturating subtract to prevent underflow
    #[inline]
    pub fn sub_bytes(&self, bytes: u64) {
        // Use a lock-free approach for saturating subtraction
        let _ = self.bytes_total.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |current| Some(current.saturating_sub(bytes))
        );
    }

    pub fn to_prometheus(&self) -> String {
        format!(
            "# HELP kv_storage_keys_total Total number of keys\n\
             # TYPE kv_storage_keys_total gauge\n\
             kv_storage_keys_total {}\n\
             # HELP kv_storage_objects_total Total unique objects\n\
             # TYPE kv_storage_objects_total gauge\n\
             kv_storage_objects_total {}\n\
             # HELP kv_storage_bytes_total Total storage bytes\n\
             # TYPE kv_storage_bytes_total gauge\n\
             kv_storage_bytes_total {}\n\
             # HELP kv_storage_ops_total Total operations\n\
             # TYPE kv_storage_ops_total counter\n\
             kv_storage_ops_total{{operation=\"put\"}} {}\n\
             kv_storage_ops_total{{operation=\"get\"}} {}\n\
             kv_storage_ops_total{{operation=\"delete\"}} {}\n\
             # HELP kv_storage_dedup_hits_total Total deduplication hits\n\
             # TYPE kv_storage_dedup_hits_total counter\n\
             kv_storage_dedup_hits_total {}\n",
            self.keys_total.load(Ordering::Relaxed),
            self.objects_total.load(Ordering::Relaxed),
            self.bytes_total.load(Ordering::Relaxed),
            self.puts_total.load(Ordering::Relaxed),
            self.gets_total.load(Ordering::Relaxed),
            self.deletes_total.load(Ordering::Relaxed),
            self.dedup_hits.load(Ordering::Relaxed)
        )
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
