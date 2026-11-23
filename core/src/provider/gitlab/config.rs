crate::provider_config! {
    pub struct GitLabConfig {
        pub url => GITLAB_URL "GitLab URL": Url [account, binds],
        pub token => GITLAB_TOKEN "GitLab Token": Password [account],
        pub user => GITLAB_USER "GitLab User": String,
    }
}
