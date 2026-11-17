pub trait ConfigSource: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}
