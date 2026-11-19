crate::provider_config! {
    pub struct JiraConfig {
        pub url => JIRA_URL: Url [account],
        pub token => JIRA_TOKEN: Password [account],
        pub insecure => JIRA_INSECURE: Bool,
    }
}
