use crate::{
    model::{CollectRequest, RangeError},
    runtime::resolve_collect_options,
};

fn request(start: &str, end: &str) -> CollectRequest {
    CollectRequest {
        start: start.to_string(),
        end: end.to_string(),
    }
}

#[test]
fn resolve_collect_options_reads_instants_with_any_offset() {
    let options = resolve_collect_options(&request(
        "2026-03-01T00:00:00+01:00",
        "2026-03-02T00:00:00Z",
    ))
    .unwrap();

    assert_eq!(options.start.to_rfc3339(), "2026-02-28T23:00:00+00:00");
    assert_eq!(options.end.to_rfc3339(), "2026-03-02T00:00:00+00:00");
}

#[test]
fn resolve_collect_options_rejects_dates_and_empty_ranges() {
    assert_eq!(
        resolve_collect_options(&request("2026-03-01", "2026-03-02")),
        Err(RangeError::NotRfc3339)
    );
    assert_eq!(
        resolve_collect_options(&request("2026-03-02T00:00:00Z", "2026-03-02T00:00:00Z")),
        Err(RangeError::Empty)
    );
}

#[test]
fn resolve_collect_options_caps_the_range_at_ten_years() {
    assert!(
        resolve_collect_options(&request("2016-01-01T00:00:00Z", "2026-01-01T00:00:00Z")).is_ok()
    );
    assert_eq!(
        resolve_collect_options(&request("1000-01-01T00:00:00Z", "9999-12-31T00:00:00Z")),
        Err(RangeError::TooLong)
    );
}
