use tauri::Url;

use super::{DeepLinkInbox, Delivery, app_link};

fn urls(raw: &[&str]) -> Vec<Url> {
    raw.iter().map(|url| Url::parse(url).unwrap()).collect()
}

#[test]
fn takes_the_first_link_of_its_own_scheme() {
    assert_eq!(
        app_link(urls(&[
            "https://example.org/day/2026-11-17",
            "activity-log://day/2026-11-17?q=standup",
            "activity-log://timeline",
        ])),
        Some("activity-log://day/2026-11-17?q=standup".to_string())
    );
}

#[test]
fn drops_links_of_other_schemes() {
    assert_eq!(
        app_link(urls(&[
            "file:///etc/passwd",
            "activity-log-evil://day/2026-11-17",
            "javascript:alert(1)",
        ])),
        None
    );
    assert_eq!(app_link(Vec::new()), None);
}

#[test]
fn holds_links_until_the_dashboard_asks_then_forwards_them() {
    let inbox = DeepLinkInbox::default();

    assert_eq!(
        inbox.receive("activity-log://timeline".to_string()),
        Delivery::Held
    );
    assert_eq!(
        inbox.receive("activity-log://day/2026-11-17".to_string()),
        Delivery::Held
    );
    assert_eq!(
        inbox.take(),
        Some("activity-log://day/2026-11-17".to_string())
    );
    assert_eq!(inbox.take(), None);
    assert_eq!(
        inbox.receive("activity-log://week/2026-11-16".to_string()),
        Delivery::Forward("activity-log://week/2026-11-16".to_string())
    );
    assert_eq!(inbox.take(), None);
}

#[test]
fn only_forwarded_links_show_the_window() {
    assert!(!Delivery::Held.shows_window());
    assert!(Delivery::Forward("activity-log://timeline".to_string()).shows_window());
}
