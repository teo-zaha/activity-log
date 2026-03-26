use std::path::PathBuf;

use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub since: NaiveDate,
    pub until: NaiveDate,
    pub output: Option<String>,
    pub git_dirs: Option<Vec<PathBuf>>,
}
