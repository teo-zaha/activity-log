use super::compare_timestamps_desc;

#[test]
fn sorts_mixed_offsets_by_instant_desc() {
    let earlier = "2026-03-10T09:30:00+01:00";
    let later = "2026-03-10T08:45:00Z";

    assert_eq!(
        compare_timestamps_desc(later, earlier),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_timestamps_desc(earlier, later),
        std::cmp::Ordering::Greater
    );
}
