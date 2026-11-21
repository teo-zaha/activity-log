use super::{FailureKind, OpenChange};
use crate::model::{StackCut, StackedChange};

fn change(title: &str, source: &str, target: &str) -> Option<OpenChange> {
    OpenChange::new(7, title.into(), source.into(), target.into(), true, None)
}

/// A blank branch would be asked as no filter at all; such a change is no change.
#[test]
fn a_change_with_a_blank_title_or_branch_is_refused() {
    assert!(change("Parse A", "feature/parse-a", "main").is_some());
    assert_eq!(change(" ", "feature/parse-a", "main"), None);
    assert_eq!(change("Parse A", "", "main"), None);
    assert_eq!(change("Parse A", "feature/parse-a", "\t"), None);
}

#[test]
fn a_change_shows_as_its_number_title_and_link() {
    let link = super::super::web_link::web_link(
        "https://gitlab.example.org/acme/widgets/-/merge_requests/7",
        &reqwest::Url::parse("https://gitlab.example.org")
            .unwrap()
            .origin(),
    );
    let change = OpenChange::new(
        7,
        "Parse A".into(),
        "feature/parse-a".into(),
        "main".into(),
        true,
        link,
    )
    .unwrap();
    assert_eq!(
        change.stacked(),
        StackedChange {
            number: 7,
            title: "Parse A".into(),
            web_url: Some("https://gitlab.example.org/acme/widgets/-/merge_requests/7".into()),
        }
    );
}

#[test]
fn each_failure_shows_as_its_cut() {
    for (failure, cut) in [
        (FailureKind::Unreadable, StackCut::Unreadable),
        (FailureKind::TokenInvalid, StackCut::Unreadable),
        (FailureKind::ScopeMissing, StackCut::Unreadable),
        (FailureKind::Throttled, StackCut::Throttled),
        (FailureKind::Limit, StackCut::Limit),
        (FailureKind::Failed, StackCut::Failed),
    ] {
        assert_eq!(failure.cut(), cut, "{failure:?}");
    }
}
