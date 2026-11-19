crate::provider_config! {
    pub struct GitLabConfig {
        pub url => GITLAB_URL: Url [account],
        pub token => GITLAB_TOKEN: Password [account],
        pub user => GITLAB_USER: String,
    }
}
