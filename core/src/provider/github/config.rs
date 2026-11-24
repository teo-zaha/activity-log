crate::provider_config! {
    pub struct GitHubConfig {
        pub url => GITHUB_URL "GitHub URL": Url [account, binds]
            placeholder "https://github.com" check GitHubWebUrl,
        pub token => GITHUB_TOKEN "GitHub Token": Password [account],
    }
}
