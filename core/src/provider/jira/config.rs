use crate::provider::fix_action::credential::{CredentialAddress, CredentialSetup, TokenPage};

crate::provider_config! {
    pub struct JiraConfig {
        pub url => JIRA_URL "Jira URL": Url [account, binds],
        pub token => JIRA_TOKEN "Jira Token": Password [account],
        pub insecure => JIRA_INSECURE "Jira Insecure": Bool [binds],
    }
}

pub(super) const CREDENTIAL_SETUP: CredentialSetup = CredentialSetup {
    token: JIRA_TOKEN,
    address: CredentialAddress::Installation(JIRA_URL),
    primary: TokenPage {
        path: &["secure", "ViewProfile.jspa"],
        parameters: &[],
        permissions: &[],
        expiry_days: None,
    },
    classic: None,
    instructions: "Open your Jira Data Center profile, select Personal access tokens, and choose Create token. The token uses your account permissions.",
};
