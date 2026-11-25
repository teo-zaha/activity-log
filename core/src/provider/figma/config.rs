use crate::provider::fix_action::credential::{
    CredentialAddress, CredentialSetup, TokenPage, TokenPermission,
};

crate::provider_config! {
    pub(crate) struct FigmaConfig {
        pub token => FIGMA_TOKEN "Figma Token": Password [account],
        pub files => FIGMA_FILES "Figma Files": [String],
        pub teams => FIGMA_TEAMS "Figma Teams": [String],
    }
}

pub(super) const CREDENTIAL_SETUP: CredentialSetup = CredentialSetup {
    token: FIGMA_TOKEN,
    address: CredentialAddress::Fixed("https://www.figma.com"),
    primary: TokenPage {
        path: &["settings"],
        parameters: &[],
        permissions: &[
            TokenPermission::Manual("current_user:read"),
            TokenPermission::Manual("file_versions:read"),
            TokenPermission::Manual("file_comments:read"),
            TokenPermission::Manual("file_metadata:read"),
            TokenPermission::Manual("folders:read for configured teams"),
        ],
        expiry_days: None,
    },
    classic: None,
    instructions: "In Figma settings, open Security and generate a personal access token. Figma limits personal access tokens to 90 days.",
};
