crate::provider_config! {
    pub struct JiraConfig {
        pub url => JIRA_URL: Url,
        pub token => JIRA_TOKEN: Password,
        pub insecure => JIRA_INSECURE: Bool,
    }
}
