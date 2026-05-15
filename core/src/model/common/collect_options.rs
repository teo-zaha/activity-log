use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub since: NaiveDate,
    pub until: NaiveDate,
}
