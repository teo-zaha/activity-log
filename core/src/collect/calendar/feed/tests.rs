use super::{FeedLocation, check_feed, feed_location, read_feed};
use crate::collect::calendar::tests::OUTLOOK_FEED;

#[test]
fn accepts_https_webcal_and_files_but_not_http() {
    assert_eq!(
        feed_location(" webcal://calendar.example.com/a.ics ").unwrap(),
        FeedLocation::Url("https://calendar.example.com/a.ics".parse().unwrap())
    );
    assert!(matches!(
        feed_location("https://outlook.office365.com/owa/calendar/x/calendar.ics").unwrap(),
        FeedLocation::Url(_)
    ));
    let file = std::env::temp_dir().join("calendar.ics");
    assert_eq!(
        feed_location(&file.display().to_string()).unwrap(),
        FeedLocation::File(file)
    );
    assert!(feed_location("http://calendar.example.com/a.ics").is_err());
    assert!(feed_location("calendar.ics").is_err());
}

#[tokio::test]
async fn errors_never_contain_the_link() {
    // Nothing listens on port 9 (discard), so the request fails fast.
    let err = read_feed("https://127.0.0.1:9/secret-token/calendar.ics")
        .await
        .unwrap_err();

    let message = format!("{err:#}");
    assert!(!message.contains("secret-token"), "{message}");
    assert!(message.contains("127.0.0.1"), "{message}");
}

#[tokio::test]
async fn reads_a_local_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.ics");
    std::fs::write(&path, OUTLOOK_FEED).unwrap();

    let text = read_feed(&path.display().to_string()).await.unwrap();

    assert!(text.starts_with("BEGIN:VCALENDAR"));
}

/// The startup check only sees whether the feed can be read; the collection reads it.
#[tokio::test]
async fn the_check_opens_the_feed_without_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.ics");
    std::fs::write(&path, OUTLOOK_FEED).unwrap();

    check_feed(&path.display().to_string()).await.unwrap();
    let missing = check_feed(&dir.path().join("gone.ics").display().to_string())
        .await
        .unwrap_err();
    assert!(format!("{missing:#}").contains("gone.ics"));
    assert!(
        check_feed(&dir.path().display().to_string()).await.is_err(),
        "a folder is no feed"
    );

    let unreachable = check_feed("https://127.0.0.1:9/secret-token/calendar.ics")
        .await
        .unwrap_err();
    let message = format!("{unreachable:#}");
    assert!(!message.contains("secret-token"), "{message}");
    assert!(message.contains("127.0.0.1"), "{message}");
    assert_eq!(
        crate::provider::source_failure::problem_of(&unreachable),
        Some(crate::model::ProviderProblem::Unreachable)
    );
}

/// A failed feed request keeps the host's answer as a typed cause.
#[tokio::test]
async fn a_failed_feed_request_names_its_problem() {
    use crate::model::ProviderProblem;
    use crate::provider::remote_api::test_server::{Reply, TestServer};
    for (status, retry_after, problem) in [
        (401, None, Some(ProviderProblem::Refused)),
        (404, None, None),
        (429, None, Some(ProviderProblem::Throttled)),
        (503, Some("30"), Some(ProviderProblem::Throttled)),
    ] {
        let server = TestServer::start(move |_| {
            let reply = Reply::json(status, "{}");
            match retry_after {
                Some(wait) => reply.with_header("Retry-After", wait),
                None => reply,
            }
        });
        // The test server speaks plain HTTP, which `feed_location` refuses: ask it directly.
        let error = super::request_feed(server.url.parse().unwrap())
            .await
            .err()
            .unwrap();
        let failure = error
            .downcast_ref::<crate::provider::remote_error::RemoteFailure>()
            .unwrap();
        assert_eq!(failure.error.problem(), problem, "{failure:?}");
        assert_eq!(failure.service, "127.0.0.1");
    }
}
