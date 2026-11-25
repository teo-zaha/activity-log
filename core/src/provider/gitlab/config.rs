use crate::provider::fix_action::credential::{
    CredentialAddress, CredentialSetup, TokenPage, TokenPermission,
};

crate::provider_config! {
    pub struct GitLabConfig {
        pub url => GITLAB_URL "GitLab URL": Url [account, binds],
        pub token => GITLAB_TOKEN "GitLab Token": Password [account],
        pub user => GITLAB_USER "GitLab User": String,
    }
}

pub(super) const CREDENTIAL_SETUP: CredentialSetup = CredentialSetup {
    token: GITLAB_TOKEN,
    address: CredentialAddress::Installation(GITLAB_URL),
    primary: TokenPage {
        path: &["-", "user_settings", "personal_access_tokens"],
        parameters: &[
            ("name", "Activity Log"),
            ("description", "Read activity for Activity Log"),
        ],
        permissions: &[TokenPermission::Scope("read_api")],
        expiry_days: None,
    },
    classic: None,
    instructions: "Create a personal access token. Choose Legacy token if GitLab offers a choice. Select the scope manually if the form leaves it unset.",
};
