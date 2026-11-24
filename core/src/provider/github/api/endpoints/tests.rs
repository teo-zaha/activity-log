use reqwest::header::AUTHORIZATION;

use crate::provider::{http_client_builder, remote_api::RemoteClient};

use super::*;

#[test]
fn derives_only_the_documented_api_endpoints() {
    for (web, user, graphql) in [
        (
            " https://GitHub.com:443/acme/widgets?what=example#issue ",
            "https://api.github.com/user",
            "https://api.github.com/graphql",
        ),
        (
            "https://Acme.ghe.com/acme/widgets",
            "https://api.acme.ghe.com/user",
            "https://api.acme.ghe.com/graphql",
        ),
        (
            "https://github.com./acme/widgets",
            "https://api.github.com/user",
            "https://api.github.com/graphql",
        ),
        (
            "https://münchen.ghe.com/",
            "https://api.xn--mnchen-3ya.ghe.com/user",
            "https://api.xn--mnchen-3ya.ghe.com/graphql",
        ),
        (
            "https://enterprise.example.org:8443/github/",
            "https://enterprise.example.org:8443/github/api/v3/user",
            "https://enterprise.example.org:8443/github/api/graphql",
        ),
        (
            "http://enterprise.example.org:8080/",
            "http://enterprise.example.org:8080/api/v3/user",
            "http://enterprise.example.org:8080/api/graphql",
        ),
        (
            "http://[::1]:8080/github",
            "http://[::1]:8080/github/api/v3/user",
            "http://[::1]:8080/github/api/graphql",
        ),
        (
            "https://github.com.example.org",
            "https://github.com.example.org/api/v3/user",
            "https://github.com.example.org/api/graphql",
        ),
        (
            "https://api.acme.ghe.com",
            "https://api.acme.ghe.com/api/v3/user",
            "https://api.acme.ghe.com/api/graphql",
        ),
    ] {
        let endpoints = Endpoints::from_url(web).unwrap();
        assert_eq!(
            endpoints.web_origin,
            Url::parse(web.trim()).unwrap().origin()
        );
        let client = RemoteClient::new(
            endpoints.base.as_str(),
            AUTHORIZATION,
            "Bearer example-token",
            "example-token",
            http_client_builder(),
        )
        .unwrap();
        assert_eq!(
            client.endpoint(endpoints.user_path()).as_str(),
            user,
            "{web}"
        );
        assert_eq!(
            client.endpoint(endpoints.graphql_path()).as_str(),
            graphql,
            "{web}"
        );
    }
}

#[test]
fn embedded_credentials_and_nonstandard_cloud_origins_are_refused() {
    for (web, problem) in [
        ("https://jdoe@example.org", SetupError::Credentials.into()),
        ("https://:pass@github.com", SetupError::Credentials.into()),
        ("http://github.com", GitHubSetupError::CloudOrigin),
        ("http://acme.ghe.com", GitHubSetupError::CloudOrigin),
        ("https://acme.ghe.com:8443", GitHubSetupError::CloudOrigin),
        (
            "https://enterprise.example.org/github/api/v3/",
            GitHubSetupError::ServerApiPath,
        ),
        (
            "https://enterprise.example.org/api/graphql",
            GitHubSetupError::ServerApiPath,
        ),
        ("ftp://example.org", SetupError::NotAnAddress.into()),
    ] {
        assert_eq!(Endpoints::from_url(web).err(), Some(problem), "{web}");
    }
}

#[test]
fn impossible_github_addresses_are_configuration_problems() {
    for error in [
        GitHubSetupError::CloudOrigin,
        GitHubSetupError::ServerApiPath,
    ] {
        assert_eq!(
            error.check_result("GitHub").outcome.problem(),
            Some(ProviderProblem::NotConfigured)
        );
    }
}
