crate::provider_config! {
    pub struct JiraConfig {
        pub url => JIRA_URL: Url,
        pub email => JIRA_EMAIL: Email,
        pub token => JIRA_TOKEN: Password,
        pub auth_type => JIRA_AUTH_TYPE: String = "bearer",
        pub insecure => JIRA_INSECURE: Bool,
    }
}
