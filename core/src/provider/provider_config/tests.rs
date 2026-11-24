use super::{ProviderConfigShape, ProviderConfigValueType};
use crate::provider::{connection::ConnectionValues, provider_context::ProviderContext};

crate::provider_config! {
    struct ExampleConfig {
        token => EXAMPLE_TOKEN "Token": Password [account],
        files => EXAMPLE_FILES "Files": [String],
    }
}

#[test]
fn macro_string_lists_preserve_unset_and_empty_values() {
    let binding = ExampleConfig::binding();
    let [token, files] = binding.requirements else {
        panic!("expected a token and a file list");
    };
    assert!(token.is_secret() && token.account);
    assert!(!files.required);
    assert_eq!(
        files.shape,
        ProviderConfigShape::List(ProviderConfigValueType::String)
    );

    for (stored, expected) in [
        (None, None),
        (Some("[]"), Some(vec![])),
        (Some(r#"["first","second"]"#), Some(vec!["first", "second"])),
    ] {
        let values = ConnectionValues::new(
            [
                (token.name.to_owned(), Some("test-secret".to_owned())),
                (files.name.to_owned(), stored.map(str::to_owned)),
            ]
            .into(),
        );
        let mut context = ProviderContext::resolve(&values, binding.requirements);
        assert!(
            context
                .missing_required_keys(binding.requirements)
                .is_empty()
        );
        (binding.resolve)(&mut context).unwrap();
        let parsed = context.config::<ExampleConfig>().unwrap();
        assert_eq!(parsed.token, "test-secret");
        assert_eq!(
            parsed.files,
            expected.map(|items| { items.into_iter().map(str::to_owned).collect() })
        );
        assert!(!format!("{parsed:?}").contains("test-secret"));
    }
}
