crate::provider_config! {
    pub struct GitLabConfig {
        pub url => GITLAB_URL: Url,
        pub token => GITLAB_TOKEN: Password,
        pub user => GITLAB_USER: String,
    }
}
