use super::FirstFailure;

#[test]
fn reports_the_first_failure_with_its_cause_and_no_other() {
    let mut failure = FirstFailure::default();

    let first = failure.report(&"No such interface supported (0x80004002)");
    assert!(
        first
            .as_deref()
            .is_some_and(|message| message.contains("0x80004002")),
        "{first:?}"
    );
    assert_eq!(failure.report(&"again"), None);
    assert_eq!(failure.report(&"and again"), None);
}
