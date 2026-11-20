use chrono::{TimeZone, Utc};

use super::Timestamp;

#[test]
fn keeps_the_text_it_was_sent() {
    let read: Timestamp = serde_json::from_str(r#""2026-11-17T08:31:00.000Z""#).unwrap();
    assert_eq!(
        serde_json::to_string(&read).unwrap(),
        r#""2026-11-17T08:31:00.000Z""#
    );
    assert_eq!(
        read.utc(),
        Utc.with_ymd_and_hms(2026, 11, 17, 8, 31, 0).unwrap()
    );
}

#[test]
fn refuses_text_that_names_no_instant() {
    for text in [
        r#""""#,
        r#""yesterday""#,
        r#""2026-11-17""#,
        r#""2026-11-17T08:00:00.000+0100""#,
    ] {
        assert!(serde_json::from_str::<Timestamp>(text).is_err(), "{text}");
    }
}

#[test]
fn orders_by_instant_then_by_text() {
    let early = Timestamp::parse("2026-11-17T09:00:00+01:00").unwrap();
    let same_in_utc = Timestamp::parse("2026-11-17T08:00:00Z").unwrap();
    let late = Timestamp::parse("2026-11-17T08:00:01Z").unwrap();
    assert!(early < late && same_in_utc < late);
    assert_eq!(early.instant(), same_in_utc.instant());
    assert_ne!(early, same_in_utc);
    // The same instant written two ways falls back to the text.
    assert!(same_in_utc < early);
}

#[test]
fn an_instant_reads_as_to_rfc3339_writes_it() {
    let at = Utc.with_ymd_and_hms(2026, 11, 17, 8, 0, 0).unwrap();
    assert_eq!(Timestamp::from(at).as_str(), at.to_rfc3339());
}
