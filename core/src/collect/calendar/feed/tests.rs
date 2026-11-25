use super::{
    FeedLocation, FeedUnavailable, check_feed, feed_location, read_feed,
    redirect::InsecureRedirect, request_feed, service_for,
};
use crate::{
    collect::calendar::tests::OUTLOOK_FEED,
    model::{CalendarFeedService, ProviderProblem},
    provider::{
        http_client_builder,
        provider_check_result::ProviderCheckResult,
        remote_api::test_server::{Reply, TestServer, tls_client_builder},
        remote_error::{ApiError, RemoteFailure},
        source_failure::FixCause,
    },
    runtime::CollectFailure,
};
use reqwest::Url;

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
    let invalid = feed_location("secret-token://calendar.example.com/a.ics").unwrap_err();
    assert!(!format!("{invalid:#}").contains("secret-token"));
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
    assert_eq!(
        ProviderCheckResult::unavailable_from(&missing)
            .outcome
            .problem(),
        Some(ProviderProblem::Failed)
    );
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
        Some(crate::provider::source_failure::ClassifiedProblem::plain(
            crate::model::ProviderProblem::Unreachable
        ))
    );
}

#[test]
fn feed_service_uses_the_original_exact_https_host() {
    use CalendarFeedService::{GoogleCalendar, OutlookCom, OutlookWork};

    for (address, expected) in [
        (
            "webcal://outlook.office365.com/secret-token.ics",
            Some(OutlookWork),
        ),
        (
            "https://outlook.office.com/secret-token.ics",
            Some(OutlookWork),
        ),
        (
            "https://outlook.cloud.microsoft/secret-token.ics",
            Some(OutlookWork),
        ),
        (
            "https://outlook.live.com/secret-token.ics",
            Some(OutlookCom),
        ),
        (
            "https://calendar.google.com:443/secret-token.ics",
            Some(GoogleCalendar),
        ),
        ("https://calendar.google.com:8443/secret-token.ics", None),
        (
            "https://user:password@calendar.google.com/secret-token.ics",
            None,
        ),
        (
            "https://calendar.google.com.example.org/secret-token.ics",
            None,
        ),
        ("https://calendar.google.com./secret-token.ics", None),
        ("https://www.icloud.com/secret-token.ics", None),
        ("https://app.fastmail.com/secret-token.ics", None),
    ] {
        let FeedLocation::Url(url) = feed_location(address).unwrap() else {
            panic!("expected a network feed");
        };
        assert_eq!(service_for(&url), expected, "{address}");
    }
    assert_eq!(
        service_for(&"http://calendar.google.com/".parse().unwrap()),
        None
    );
}

/// Both entry points classify the same typed request failure through their own outcome paths.
#[tokio::test]
async fn a_failed_feed_request_names_its_problem() {
    use ProviderProblem::{CollectionFailed, Failed, Throttled};

    for (status, header, expected) in [
        (401, None, ProviderProblem::FeedUnavailable),
        (403, None, ProviderProblem::FeedUnavailable),
        (404, None, ProviderProblem::FeedUnavailable),
        (
            403,
            Some(("WWW-Authenticate", r#"Bearer error="insufficient_scope""#)),
            ProviderProblem::FeedUnavailable,
        ),
        (403, Some(("X-RateLimit-Remaining", "0")), Throttled),
        (403, Some(("Retry-After", "30")), Throttled),
        (429, None, Throttled),
        (503, Some(("Retry-After", "30")), Throttled),
        (500, None, Failed),
    ] {
        let server = TestServer::start(move |_| {
            let reply = Reply::json(status, r#"{"message":"echoed-secret"}"#);
            match header {
                Some((name, value)) => reply.with_header(name, value),
                None => reply,
            }
        });
        // The test server speaks plain HTTP, which `feed_location` refuses: ask it directly.
        let url = format!("{}/feed/path-secret.ics?key=query-secret", server.url);
        let error = request_feed(url.parse().unwrap(), http_client_builder())
            .await
            .unwrap_err();
        let check = ProviderCheckResult::unavailable_from(&error).outcome;
        let collection = CollectFailure::of(&error);
        assert_eq!(check.problem(), Some(expected));
        assert_eq!(
            collection.problem(),
            if expected == Failed {
                CollectionFailed
            } else {
                expected
            }
        );

        let failure = if expected == ProviderProblem::FeedUnavailable {
            let unavailable = error.downcast_ref::<FeedUnavailable>().unwrap();
            assert_eq!(
                check.classification().unwrap().cause(),
                Some(FixCause::FeedUnavailable(None))
            );
            assert_eq!(
                collection.classification().cause(),
                Some(FixCause::FeedUnavailable(None))
            );
            &unavailable.failure
        } else {
            assert!(check.classification().unwrap().cause().is_none());
            error.downcast_ref::<RemoteFailure>().unwrap()
        };
        assert!(
            matches!(failure.error, ApiError::Status { status: actual, .. } if actual.as_u16() == status)
        );
        assert_eq!(failure.service, "The calendar link on 127.0.0.1");
        for text in [check.reason().unwrap(), collection.message()] {
            assert!(
                text.contains(&format!("The calendar link on 127.0.0.1 answered {status}")),
                "{text}"
            );
            for secret in ["path-secret", "query-secret", "echoed-secret"] {
                assert!(!text.contains(secret), "{text}");
            }
        }
    }
}

#[tokio::test]
async fn redirected_feed_failures_keep_the_original_host_and_omit_referrers() {
    for other_origin in [false, true] {
        let target = TestServer::start(|_| Reply::json(404, "{}"));
        let location = if other_origin {
            format!("http://localhost:{}/redirect-secret.ics", target.port)
        } else {
            "/redirect-secret.ics".to_owned()
        };
        let server = TestServer::start(move |request| {
            if request.path() == "/redirect-secret.ics" {
                Reply::json(404, "{}")
            } else {
                Reply::redirect(location.clone())
            }
        });
        let address = format!("{}/start-secret.ics?key=query-secret", server.url);
        let error = request_feed(address.parse().unwrap(), http_client_builder())
            .await
            .unwrap_err();
        let failure = error.downcast_ref::<FeedUnavailable>().unwrap();
        assert_eq!(failure.service, None);
        assert_eq!(failure.failure.service, "The calendar link on 127.0.0.1");
        let message = format!("{error:#}");
        assert!(
            message.contains("The calendar link on 127.0.0.1 answered 404"),
            "{message}"
        );
        for secret in [
            "localhost",
            "redirect-secret",
            "start-secret",
            "query-secret",
        ] {
            assert!(!message.contains(secret), "{message}");
        }
        let requests = if other_origin {
            target.requests()
        } else {
            server.requests()
        };
        let followed = requests.last().unwrap();
        assert_eq!(followed.path(), "/redirect-secret.ics");
        assert_eq!(followed.header("referer"), None);
    }
}

#[tokio::test]
async fn a_calendar_downgrade_is_refused_at_any_hop_without_leaking_addresses() {
    let plaintext = TestServer::start(|_| Reply::html(200, OUTLOOK_FEED));
    let mut target = Url::parse(&plaintext.url)
        .unwrap()
        .join("/target-path-secret?key=target-query-secret")
        .unwrap();
    target.set_host(Some("localhost")).unwrap();
    let https = TestServer::start_tls(move |request| match request.path() {
        "/chain-path-secret" => {
            Reply::redirect("/origin-path-secret?key=intermediate-query-secret")
        }
        _ => Reply::redirect(target.as_str()),
    });
    let tls = Url::parse(&https.url).unwrap();
    let direct = tls
        .join("/origin-path-secret?key=origin-query-secret")
        .unwrap();
    let upgrade = {
        let direct = direct.clone();
        TestServer::start(move |_| Reply::redirect(direct.as_str()))
    };

    for start in [
        direct,
        tls.join("/chain-path-secret?key=origin-query-secret")
            .unwrap(),
        Url::parse(&upgrade.url)
            .unwrap()
            .join("/initial-path-secret?key=origin-query-secret")
            .unwrap(),
    ] {
        let answer = request_feed(start, tls_client_builder()).await;
        assert!(
            plaintext.requests().is_empty(),
            "the redirect target received a request"
        );
        let error = answer.unwrap_err();
        assert!(error.is::<InsecureRedirect>(), "{error:#}");
        let check = ProviderCheckResult::unavailable_from(&error).outcome;
        let collection = CollectFailure::of(&error);
        assert_eq!(check.problem(), Some(ProviderProblem::Failed));
        assert_eq!(collection.problem(), ProviderProblem::CollectionFailed);
        let [cause] = error.chain().collect::<Vec<_>>()[..] else {
            panic!("the sanitized error must not retain reqwest's error chain");
        };
        let expected = "The calendar server at 127.0.0.1 redirected to an insecure (http) address. Activity Log refused it.";
        assert_eq!(cause.to_string(), expected);
        assert_eq!(check.reason(), Some(expected));
        assert_eq!(collection.message(), expected);
        for secret in [
            "localhost",
            "target-path-secret",
            "target-query-secret",
            "chain-path-secret",
            "origin-path-secret",
            "initial-path-secret",
            "origin-query-secret",
            "intermediate-query-secret",
        ] {
            assert!(!format!("{error:?}").contains(secret));
        }
    }
    assert_eq!(
        https
            .requests()
            .iter()
            .map(|request| request.path())
            .collect::<Vec<_>>(),
        [
            "/origin-path-secret",
            "/chain-path-secret",
            "/origin-path-secret",
            "/origin-path-secret"
        ]
    );
    assert_eq!(upgrade.requests().len(), 1);
}

#[tokio::test]
async fn calendar_follows_upgrades_and_same_scheme_redirects_with_the_default_limit() {
    let https = TestServer::start_tls(|request| match request.path() {
        "/before" => Reply::redirect("/feed"),
        "/loop" => Reply::redirect("/loop"),
        _ => Reply::html(200, OUTLOOK_FEED),
    });
    let tls = Url::parse(&https.url).unwrap();
    let http = {
        let target = tls.join("/before").unwrap();
        TestServer::start(move |request| match request.path() {
            "/" => Reply::redirect("/upgrade"),
            _ => Reply::redirect(target.as_str()),
        })
    };
    let (_, response) = request_feed(Url::parse(&http.url).unwrap(), tls_client_builder())
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), OUTLOOK_FEED);
    assert_eq!(http.requests().len(), 2);
    assert_eq!(
        https
            .requests()
            .iter()
            .map(|request| request.path())
            .collect::<Vec<_>>(),
        ["/before", "/feed"]
    );

    let error = request_feed(tls.join("/loop").unwrap(), tls_client_builder())
        .await
        .unwrap_err();
    assert!(error.is::<RemoteFailure>());
    assert_eq!(
        https
            .requests()
            .iter()
            .filter(|request| request.path() == "/loop")
            .count(),
        11,
        "the initial request plus reqwest's ten allowed redirects"
    );
}
