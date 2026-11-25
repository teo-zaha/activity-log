use crate::provider::fix_action::credential::{
    CredentialAddress, CredentialSetup, TokenPage, TokenPermission,
};

crate::provider_config! {
    pub struct GitHubConfig {
        pub url => GITHUB_URL "GitHub URL": Url [account, binds]
            placeholder "https://github.com" check GitHubWebUrl,
        pub token => GITHUB_TOKEN "GitHub Token": Password [account],
    }
}

const GITHUB_PAT_EXPIRY_DAYS: u16 = 365;

pub(super) const CREDENTIAL_SETUP: CredentialSetup = CredentialSetup {
    token: GITHUB_TOKEN,
    address: CredentialAddress::Origin(GITHUB_URL),
    primary: TokenPage {
        path: &["settings", "personal-access-tokens", "new"],
        parameters: &[
            ("name", "Activity Log"),
            ("description", "Read activity for Activity Log"),
        ],
        permissions: &[
            TokenPermission::ReadOnly {
                name: "Metadata",
                key: "metadata",
            },
            TokenPermission::ReadOnly {
                name: "Pull requests",
                key: "pull_requests",
            },
            TokenPermission::ReadOnly {
                name: "Issues",
                key: "issues",
            },
            TokenPermission::ReadOnly {
                name: "Contents",
                key: "contents",
            },
        ],
        expiry_days: Some(GITHUB_PAT_EXPIRY_DAYS),
    },
    classic: Some(TokenPage {
        path: &["settings", "tokens", "new"],
        parameters: &[("description", "Activity Log")],
        permissions: &[TokenPermission::Scope("repo")],
        expiry_days: None,
    }),
    instructions: "A fine-grained token covers one owner. Choose the repositories to include and grant read-only access. Follow any GitHub approval or SSO requirements. If your organization sets a shorter token lifetime, GitHub applies that limit in its form. The classic token grants broader access.",
};
