crate::provider_config! {
    pub struct JiraConfig {
        pub url => JIRA_URL "Jira URL": Url [account, binds],
        pub token => JIRA_TOKEN "Jira Token": Password [account],
        pub insecure => JIRA_INSECURE "Jira Insecure": Bool [binds],
    }
}
