use std::{
    env, fs,
    future::Future,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::model::CollectOptions;

const CACHE_DIR: &str = ".cache";

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile<T> {
    saved_at: u64,
    data: T,
}

pub fn get_cached<T>(key: &str) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let file_path = cache_file_path(key);
    if !file_path.exists() {
        return Ok(None);
    }

    let raw = match fs::read_to_string(&file_path) {
        Ok(raw) => raw,
        Err(_) => return Ok(None),
    };

    let cached: CacheFile<T> = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let saved_at = UNIX_EPOCH
        .checked_add(Duration::from_millis(cached.saved_at))
        .unwrap_or(UNIX_EPOCH);
    if is_expired(saved_at, ttl()) {
        return Ok(None);
    }

    Ok(Some(cached.data))
}

pub fn set_cached<T>(key: &str, data: &T) -> Result<()>
where
    T: Serialize,
{
    let dir = Path::new(CACHE_DIR);
    fs::create_dir_all(dir).with_context(|| format!("creating cache dir {}", dir.display()))?;

    let payload = CacheFile {
        saved_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        data,
    };
    let json = serde_json::to_string(&payload)?;
    fs::write(cache_file_path(key), json)?;
    Ok(())
}

pub async fn with_range_cache<T, F, Fut, H>(
    source: &str,
    options: &CollectOptions,
    on_hit: H,
    load: F,
) -> Result<T>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
    H: FnOnce(&T),
{
    let cache_key = range_cache_key(source, options);
    if let Some(cached) = get_cached::<T>(&cache_key)? {
        on_hit(&cached);
        return Ok(cached);
    }

    let data = load().await?;
    set_cached(&cache_key, &data)?;
    Ok(data)
}

fn cache_file_path(key: &str) -> PathBuf {
    let safe = key
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect::<String>();
    Path::new(CACHE_DIR).join(format!("{safe}.json"))
}

fn range_cache_key(source: &str, options: &CollectOptions) -> String {
    format!(
        "{source}-{}-{}",
        options.since.format("%Y-%m-%d"),
        options.until.format("%Y-%m-%d")
    )
}

fn ttl() -> Duration {
    let minutes = env::var("CACHE_TTL_MINUTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    Duration::from_secs(minutes * 60)
}

fn is_expired(saved_at: SystemTime, ttl: Duration) -> bool {
    SystemTime::now()
        .duration_since(saved_at)
        .map(|elapsed| elapsed > ttl)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
