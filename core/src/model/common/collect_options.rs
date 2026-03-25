use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub since: String,
    pub until: String,
    pub output: Option<String>,
    pub git_dirs: Option<Vec<PathBuf>>,
}
