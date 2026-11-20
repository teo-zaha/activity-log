use super::activity_typescript;
use crate::model::CollectOptions;

#[test]
fn holds_the_range_limit_the_backend_checks() {
    let output = activity_typescript();

    assert!(output.contains(&format!(
        "export const COLLECT_MAX_DAYS = {}\n",
        CollectOptions::MAX_DAYS
    )));
    assert!(output.ends_with('\n') && !output.ends_with("\n\n"));
}
