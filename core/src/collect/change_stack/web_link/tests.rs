use reqwest::Url;

use super::web_link;

fn origin(url: &str) -> url::Origin {
    Url::parse(url).unwrap().origin()
}

#[test]
fn a_link_to_the_hosts_pages_is_kept() {
    let allowed = origin("https://gitlab.example.org/gitlab");
    for link in [
        "https://gitlab.example.org/acme/widgets/-/merge_requests/12",
        "https://GITLAB.example.org:443/acme/widgets/-/merge_requests/12",
        "https://gitlab.example.org/other/place?tab=diffs#note_1",
    ] {
        assert!(web_link(link, &allowed).is_some(), "{link}");
    }
}

#[test]
fn every_other_link_is_dropped() {
    let allowed = origin("https://gitlab.example.org");
    for link in [
        "https://evil.example.org/acme/widgets/-/merge_requests/12",
        "https://u:p@gitlab.example.org/acme/widgets/-/merge_requests/12",
        "https://u@gitlab.example.org/acme/widgets/-/merge_requests/12",
        "https://:p@gitlab.example.org/acme/widgets/-/merge_requests/12",
        "http://gitlab.example.org/acme/widgets/-/merge_requests/12",
        "https://gitlab.example.org:8443/acme/widgets/-/merge_requests/12",
        "javascript:alert(1)",
        "data:text/html,<b>hi</b>",
        "/acme/widgets/-/merge_requests/12",
        "gitlab.example.org/acme",
        "",
    ] {
        assert_eq!(web_link(link, &allowed), None, "{link}");
    }
}

/// An http host's pages are http; its https address is another origin.
#[test]
fn http_and_https_are_different_origins() {
    let allowed = origin("http://gitlab.example.org:8080");
    assert!(web_link("http://gitlab.example.org:8080/acme/widgets", &allowed).is_some());
    assert_eq!(
        web_link("https://gitlab.example.org:8080/acme/widgets", &allowed),
        None
    );
}

/// The link is kept as the URL parser writes it, so what is shown is what was checked.
#[test]
fn the_link_is_kept_as_parsed() {
    let allowed = origin("https://gitlab.example.org");
    let link = web_link("https://GitLab.Example.org/a b", &allowed).unwrap();
    assert_eq!(link.as_str(), "https://gitlab.example.org/a%20b");
}
