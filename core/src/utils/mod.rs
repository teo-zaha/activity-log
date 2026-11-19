use chrono::Utc;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// An id for an entry whose source has none: FNV-1a over `parts`, which stays the same across
/// runs and Rust versions (unlike `DefaultHasher`).
pub fn stable_id(parts: &[&str]) -> String {
    let bytes = parts.iter().enumerate().flat_map(|(index, part)| {
        let separator = (index > 0).then_some(0x1f_u8);
        separator.into_iter().chain(part.bytes())
    });
    format!("h:{:016x}", fnv1a(bytes))
}

/// FNV-1a, 64 bits.
pub fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A random number for names that must not repeat (staging folders, connection instances).
/// Not for secrets.
pub fn random_u64() -> u64 {
    use std::{
        hash::{BuildHasher, Hasher},
        sync::atomic::{AtomicU64, Ordering},
    };
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // `RandomState` is seeded from the OS once per thread and changes with each use.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(now.as_nanos());
    }
    hasher.finish()
}
