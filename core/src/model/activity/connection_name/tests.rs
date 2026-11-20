use super::ConnectionName;
use crate::model::ConnectionStamp;

#[test]
fn an_empty_name_is_refused_and_an_unstamped_one_never_sent() {
    assert!(serde_json::from_str::<ConnectionName>(r#""""#).is_err());
    assert_eq!(
        serde_json::from_str::<ConnectionName>(r#""work""#).unwrap(),
        "work"
    );
    assert!(serde_json::to_string(&ConnectionName::unstamped()).is_err());

    assert_eq!(ConnectionStamp::new(""), None);
    let mut stamped = ConnectionName::unstamped();
    stamped.stamp(&"work".into());
    assert_eq!(serde_json::to_string(&stamped).unwrap(), r#""work""#);
}
