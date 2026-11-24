crate::provider_config! {
    pub(crate) struct FigmaConfig {
        pub token => FIGMA_TOKEN "Figma Token": Password [account],
        pub files => FIGMA_FILES "Figma Files": [String],
    }
}
