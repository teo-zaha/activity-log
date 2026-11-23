use std::collections::BTreeMap;

use super::{
    Binding, BindingError, KeyId, Malformed, SealError, SealedValue, Unopenable, open, seal,
};
use crate::{
    model::Source,
    provider::{
        adapter,
        calendar::CALENDAR_ICS_URL,
        connection::ConnectionValues,
        gitlab::{GITLAB_TOKEN, GITLAB_URL, GITLAB_USER},
        jira::JIRA_TOKEN,
        provider_config::{ProviderConfigKey, ProviderConfigValueType},
        provider_registry,
    },
    secrets::{DataKey, KEY_LEN, StoreKind},
};

const TOKEN: &str = "glpat-test-0000";

fn key(byte: u8) -> DataKey {
    DataKey::from_slice(&[byte; KEY_LEN]).unwrap()
}

fn keys_of(source: Source) -> &'static [ProviderConfigKey] {
    adapter(&source).config_requirements()
}

fn values(pairs: &[(&str, &str)]) -> ConnectionValues {
    ConnectionValues::new(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), Some(value.to_string())))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn gitlab(url: &str) -> Binding {
    Binding::of(
        keys_of(Source::Gitlab),
        &values(&[("GITLAB_URL", url), ("GITLAB_USER", "jane")]),
    )
    .unwrap()
}

fn jira(url: &str, insecure: Option<&str>) -> Binding {
    let mut pairs = vec![("JIRA_URL", url)];
    pairs.extend(insecure.map(|insecure| ("JIRA_INSECURE", insecure)));
    Binding::of(keys_of(Source::Jira), &values(&pairs)).unwrap()
}

fn sealed_token(binding: &Binding) -> SealedValue {
    seal(&key(1), StoreKind::Os, GITLAB_TOKEN, binding, TOKEN).unwrap()
}

#[test]
fn a_sealed_value_opens_as_it_was_and_reads_back_from_its_text() {
    let binding = gitlab("https://gitlab.example.test");
    let sealed = sealed_token(&binding);
    assert!(sealed.as_str().starts_with("v1:os:"));
    let read = SealedValue::parse(sealed.as_str()).unwrap();
    assert_eq!(read, sealed);
    assert_eq!(read.store(), StoreKind::Os);
    assert_eq!(read.key_id(), KeyId::of(&key(1)));
    for value in [&sealed, &read] {
        let opened = open(value, &key(1), GITLAB_TOKEN, &binding).unwrap();
        assert_eq!(opened.as_str(), TOKEN);
    }
    // Short and long secrets alike (S2V pads one shorter than a block).
    for secret in ["x", "0123456789abcde", "0123456789abcdef", &"y".repeat(400)] {
        let sealed = seal(&key(1), StoreKind::File, GITLAB_TOKEN, &binding, secret).unwrap();
        assert!(sealed.as_str().starts_with("v1:file:"));
        let opened = open(&sealed, &key(1), GITLAB_TOKEN, &binding).unwrap();
        assert_eq!(opened.as_str(), secret);
    }
}

/// The same key, secret, field and binding give the same text, so a token entered again is
/// no change; anything else differs.
#[test]
fn sealing_is_deterministic_and_nothing_else() {
    let binding = gitlab("https://gitlab.example.test");
    let once = sealed_token(&binding);
    assert_eq!(sealed_token(&binding).as_str(), once.as_str());
    let others = [
        seal(&key(2), StoreKind::Os, GITLAB_TOKEN, &binding, TOKEN).unwrap(),
        seal(&key(1), StoreKind::File, GITLAB_TOKEN, &binding, TOKEN).unwrap(),
        seal(
            &key(1),
            StoreKind::Os,
            GITLAB_TOKEN,
            &binding,
            "glpat-test-0001",
        )
        .unwrap(),
        seal(&key(1), StoreKind::Os, JIRA_TOKEN, &binding, TOKEN).unwrap(),
        sealed_token(&gitlab("https://other.example.test")),
    ];
    for other in others {
        assert_ne!(other.as_str(), once.as_str());
    }
}

/// The format is pinned: a value sealed today opens in every later version. Each vector was
/// computed independently from the ADR's definitions (SHA-256 for the ids, pyca/cryptography's
/// AES-SIV over OpenSSL with the same associated data), and matches byte for byte.
#[test]
fn the_v1_text_is_pinned() {
    let jira_binding = Binding::of(
        keys_of(Source::Jira),
        &values(&[
            ("JIRA_URL", "https://jira.example.test:8443/"),
            ("JIRA_INSECURE", "true"),
        ]),
    )
    .unwrap();
    let calendar_binding = Binding::of(keys_of(Source::Calendar), &values(&[])).unwrap();
    let vectors = [
        (
            key(1),
            StoreKind::Os,
            GITLAB_TOKEN,
            gitlab("https://gitlab.example.test"),
            TOKEN,
            "v1:os:2c9b4508:e5add042:Z6q9UyLkNkvLurag57DDwPh66TvP6jkJwkov3dxk6g",
        ),
        (
            key(2),
            StoreKind::File,
            JIRA_TOKEN,
            jira_binding,
            "jira-test-0000",
            "v1:file:41b9d50f:a3c01429:8mvxnQBkaW7ZNZV2bbmiM3tsUfvkkSz5EqGKWLfY",
        ),
        (
            key(3),
            StoreKind::Os,
            CALENDAR_ICS_URL,
            calendar_binding,
            "https://calendar.example.test/private/feed.ics",
            "v1:os:bc8dc943:058f5011:aTjmNSpSOhN5i5ANcn3mchrwvNUoPE3LgWOFxn1QApcpu4dW11TLZgr-lkst-7353Tyeelbx6dUx-73UHmM",
        ),
    ];
    for (key, store, field, binding, secret, pinned) in vectors {
        assert_eq!(
            seal(&key, store, field, &binding, secret).unwrap().as_str(),
            pinned
        );
        let read = SealedValue::parse(pinned).unwrap();
        assert_eq!(open(&read, &key, field, &binding).unwrap().as_str(), secret);
    }
}

#[test]
fn another_key_is_told_before_anything_is_decrypted() {
    let binding = gitlab("https://gitlab.example.test");
    let sealed = sealed_token(&binding);
    assert_eq!(
        open(&sealed, &key(2), GITLAB_TOKEN, &binding),
        Err(Unopenable::OtherKey)
    );
    // Even for another server: the key decides first.
    assert_eq!(
        open(
            &sealed,
            &key(2),
            GITLAB_TOKEN,
            &gitlab("https://other.example.test")
        ),
        Err(Unopenable::OtherKey)
    );
}

/// A token sealed for one server opens only for that server's origin: its spelling, path and
/// query do not count, its scheme, host and port do.
#[test]
fn a_token_opens_only_for_its_origin() {
    let sealed = sealed_token(&gitlab("https://gitlab.example.test"));
    for same in [
        "https://gitlab.example.test:443",
        "HTTPS://GitLab.Example.Test/",
        "https://gitlab.example.test/group/sub?x=1#top",
        " https://gitlab.example.test ",
    ] {
        let opened = open(&sealed, &key(1), GITLAB_TOKEN, &gitlab(same)).unwrap();
        assert_eq!(opened.as_str(), TOKEN, "{same}");
    }
    for other in [
        "http://gitlab.example.test",
        "https://gitlab.example.test:8443",
        "https://evil.example.test",
        "https://gitlab.example.test.evil.example",
    ] {
        assert_eq!(
            open(&sealed, &key(1), GITLAB_TOKEN, &gitlab(other)),
            Err(Unopenable::OtherAddress),
            "{other}"
        );
    }
}

/// Jira's token is bound to its TLS setting too; unset and unreadable read as `false`.
#[test]
fn a_jira_token_opens_only_with_its_tls_setting() {
    let url = "https://jira.example.test";
    let sealed = seal(&key(1), StoreKind::Os, JIRA_TOKEN, &jira(url, None), TOKEN).unwrap();
    for same in [None, Some("false"), Some(" FALSE "), Some("no")] {
        assert!(
            open(&sealed, &key(1), JIRA_TOKEN, &jira(url, same)).is_ok(),
            "{same:?}"
        );
    }
    for other in [Some("true"), Some("True")] {
        assert_eq!(
            open(&sealed, &key(1), JIRA_TOKEN, &jira(url, other)),
            Err(Unopenable::OtherAddress),
            "{other:?}"
        );
    }
}

/// A value whose key and binding match and that still does not open is damaged: a flipped
/// byte, an edited header, or a binding tag edited to match the file's new server.
#[test]
fn an_edited_value_is_damaged() {
    let binding = gitlab("https://gitlab.example.test");
    let sealed = sealed_token(&binding);
    let text = sealed.as_str();
    let (head, payload) = text.rsplit_once(':').unwrap();

    let mut bytes = base64_decode(payload);
    bytes[20] ^= 1;
    let flipped = SealedValue::parse(&format!("{head}:{}", base64_encode(&bytes))).unwrap();
    assert_eq!(
        open(&flipped, &key(1), GITLAB_TOKEN, &binding),
        Err(Unopenable::Damaged)
    );

    // Another store named in the header: authenticated, so it does not open.
    let restored = SealedValue::parse(&text.replacen("v1:os:", "v1:file:", 1)).unwrap();
    assert_eq!(
        open(&restored, &key(1), GITLAB_TOKEN, &binding),
        Err(Unopenable::Damaged)
    );

    // The file points elsewhere, and the tag was edited to match it.
    let elsewhere = gitlab("https://evil.example.test");
    let old_tag = binding.tag().to_string();
    let new_tag = elsewhere.tag().to_string();
    let retagged = SealedValue::parse(&text.replacen(&old_tag, &new_tag, 1)).unwrap();
    assert_eq!(
        open(&retagged, &key(1), GITLAB_TOKEN, &elsewhere),
        Err(Unopenable::Damaged)
    );
}

/// A value moved to another secret field does not open there.
#[test]
fn a_value_moved_to_another_field_does_not_open() {
    let binding = Binding::of(keys_of(Source::Calendar), &values(&[])).unwrap();
    let sealed = seal(
        &key(1),
        StoreKind::Os,
        CALENDAR_ICS_URL,
        &binding,
        "https://calendar.example.test/private/feed.ics",
    )
    .unwrap();
    assert!(open(&sealed, &key(1), CALENDAR_ICS_URL, &binding).is_ok());
    assert_eq!(
        open(&sealed, &key(1), GITLAB_TOKEN, &binding),
        Err(Unopenable::Damaged)
    );
}

#[test]
fn only_a_non_empty_secret_is_sealed() {
    let binding = gitlab("https://gitlab.example.test");
    for field in [GITLAB_URL, GITLAB_USER] {
        assert_eq!(
            seal(&key(1), StoreKind::Os, field, &binding, "x"),
            Err(SealError::NotASecret)
        );
    }
    assert_eq!(
        seal(&key(1), StoreKind::Os, GITLAB_TOKEN, &binding, ""),
        Err(SealError::Empty)
    );
}

/// A binding needs an origin; without one nothing is sealed or opened.
#[test]
fn a_binding_needs_an_origin() {
    for url in [
        None,
        Some(""),
        Some("not a url"),
        Some("mailto:jane@example.test"),
    ] {
        let mut pairs = vec![("GITLAB_USER", "jane")];
        pairs.extend(url.map(|url| ("GITLAB_URL", url)));
        assert_eq!(
            Binding::of(keys_of(Source::Gitlab), &values(&pairs)),
            Err(BindingError::NoOrigin { key: "GITLAB_URL" }),
            "{url:?}"
        );
    }
}

/// Hosts are compared as `url` serializes them: IDNA as ASCII, IPv6 in brackets, the port
/// always written.
#[test]
fn origins_are_normalized() {
    let tag = |url: &str| gitlab(url).tag();
    assert_eq!(
        tag("https://bücher.example.test"),
        tag("https://xn--bcher-kva.example.test:443")
    );
    assert_eq!(tag("https://[::1]"), tag("https://[0:0::1]:443"));
    assert_eq!(
        tag("http://gitlab.example.test"),
        tag("http://gitlab.example.test:80")
    );
    assert_ne!(
        tag("http://gitlab.example.test:443"),
        tag("https://gitlab.example.test")
    );
    // The binding names only the `[binds]` keys: the user does not count.
    let other_user = Binding::of(
        keys_of(Source::Gitlab),
        &values(&[
            ("GITLAB_URL", "https://gitlab.example.test"),
            ("GITLAB_USER", "joe"),
        ]),
    )
    .unwrap();
    assert_eq!(other_user.tag(), tag("https://gitlab.example.test"));
}

#[test]
fn malformed_texts_are_told_apart() {
    let good = sealed_token(&gitlab("https://gitlab.example.test"));
    let parts: Vec<&str> = good.as_str().split(':').collect();
    let with = |index: usize, part: &str| {
        let mut parts = parts.clone();
        parts[index] = part;
        parts.join(":")
    };
    for (text, malformed) in [
        (String::new(), Malformed::Shape),
        ("glpat-plain-text".to_string(), Malformed::Shape),
        (format!("{}:extra", good.as_str()), Malformed::Shape),
        (with(0, "v2"), Malformed::Version),
        (with(1, "keychain"), Malformed::Store),
        (with(2, "9B2E41C0"), Malformed::KeyId),
        (with(2, "9b2e41"), Malformed::KeyId),
        (with(3, "zzzzzzzz"), Malformed::BindingTag),
        (with(4, "AAAA"), Malformed::Payload),
        (with(4, &format!("{}=", parts[4])), Malformed::Payload),
        (with(4, "not base64!"), Malformed::Payload),
    ] {
        assert_eq!(SealedValue::parse(&text).err(), Some(malformed), "{text}");
    }
}

#[test]
fn nothing_secret_is_printed() {
    let sealed = sealed_token(&gitlab("https://gitlab.example.test"));
    let debug = format!("{sealed:?}");
    let payload = sealed.as_str().rsplit_once(':').unwrap().1;
    assert!(!debug.contains(payload), "{debug}");
    assert!(debug.contains(&KeyId::of(&key(1)).to_string()), "{debug}");
    assert!(!format!("{:?}", key(1)).contains("01"));
}

/// Every `[binds]` key binds by a type a binding reads (a URL's origin, a bool).
#[test]
fn every_binding_key_binds_by_its_type() {
    for adapter in provider_registry() {
        for key in adapter.config_requirements().iter().filter(|key| key.binds) {
            assert!(
                matches!(
                    key.value_type(),
                    ProviderConfigValueType::Url | ProviderConfigValueType::Bool
                ),
                "{}",
                key.name
            );
        }
    }
}

fn base64_decode(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .unwrap()
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
