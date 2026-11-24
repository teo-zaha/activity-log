use std::collections::BTreeMap;

use super::{
    Binding, BindingError, KeyId, Malformed, SealError, SealedValue, Unopenable, open, seal,
};
use crate::{
    model::Source,
    provider::{
        calendar::CALENDAR_ICS_URL,
        connection::ConnectionValues,
        figma::FIGMA_TOKEN,
        github::GITHUB_TOKEN,
        gitlab::{GITLAB_TOKEN, GITLAB_URL, GITLAB_USER},
        jira::JIRA_TOKEN,
        provider_config::ProviderConfigValueType,
        provider_registry,
    },
    secrets::{DataKey, KEY_LEN, StoreKind},
};

const TOKEN: &str = "glpat-test-0000";

fn key(byte: u8) -> DataKey {
    DataKey::from_slice(&[byte; KEY_LEN]).unwrap()
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
        &Source::Gitlab,
        &values(&[("GITLAB_URL", url), ("GITLAB_USER", "jane")]),
    )
    .unwrap()
}

fn jira(url: &str, insecure: Option<&str>) -> Binding {
    let mut pairs = vec![("JIRA_URL", url)];
    pairs.extend(insecure.map(|insecure| ("JIRA_INSECURE", insecure)));
    Binding::of(&Source::Jira, &values(&pairs)).unwrap()
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
        sealed_token(&gitlab("https://other.example.test")),
    ];
    for other in others {
        assert_ne!(other.as_str(), once.as_str());
    }
}

/// The format is pinned: a value sealed today opens in every later version. Each vector was
/// computed (or, for the review's two, opened) independently from the ADR's definitions (SHA-256 for the ids, pyca/cryptography's
/// AES-SIV over OpenSSL with the same associated data), and matches byte for byte.
#[test]
fn the_v1_text_is_pinned() {
    let jira_binding = Binding::of(
        &Source::Jira,
        &values(&[
            ("JIRA_URL", "https://jira.example.test:8443/"),
            ("JIRA_INSECURE", "true"),
        ]),
    )
    .unwrap();
    let calendar_binding = Binding::of(&Source::Calendar, &values(&[])).unwrap();
    // The review's own two, the secrets recovered by opening its texts with OpenSSL.
    let review_jira = Binding::of(
        &Source::Jira,
        &values(&[
            ("JIRA_URL", "HTTPS://Bücher.Example.Test:443/jira/"),
            ("JIRA_INSECURE", " False "),
        ]),
    )
    .unwrap();
    let review_gitlab = gitlab("http://[2001:DB8::1]:8080/x");
    let vectors = [
        (
            key(4),
            StoreKind::File,
            JIRA_TOKEN,
            review_jira,
            "jira-täst-ünïcode",
            "v1:file:d7b0e2a4:7f19d6e3:ye-6PqiD_AS29YKR16cisUXz5cpfCSbV7wgFzAqrejy2_8Fi",
        ),
        (
            key(5),
            StoreKind::Os,
            GITLAB_TOKEN,
            review_gitlab,
            "0123456789abcdef",
            "v1:os:78817e8a:5be3e64f:BroLa9RzhBZYD4jBqwg7jmeqSE0nl4pRckrqYQVXbKg",
        ),
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

/// A binding seals and opens only its own kind's fields; a value pasted into another kind's
/// table names another binding there.
#[test]
fn a_binding_takes_only_its_kinds_fields() {
    let gitlab_binding = gitlab("https://gitlab.example.test");
    assert_eq!(
        seal(&key(1), StoreKind::Os, JIRA_TOKEN, &gitlab_binding, TOKEN),
        Err(SealError::OtherKind)
    );
    let sealed = sealed_token(&gitlab_binding);
    assert_eq!(
        open(&sealed, &key(1), JIRA_TOKEN, &gitlab_binding),
        Err(Unopenable::OtherKind)
    );
    assert_eq!(
        open(
            &sealed,
            &key(1),
            JIRA_TOKEN,
            &jira("https://gitlab.example.test", None)
        ),
        Err(Unopenable::OtherAddress)
    );
}

/// A value moved to another secret field does not open there.
#[test]
fn a_value_moved_to_another_field_does_not_open() {
    let binding = Binding::of(&Source::Calendar, &values(&[])).unwrap();
    let sealed = seal(
        &key(1),
        StoreKind::Os,
        CALENDAR_ICS_URL,
        &binding,
        "https://calendar.example.test/private/feed.ics",
    )
    .unwrap();
    assert!(open(&sealed, &key(1), CALENDAR_ICS_URL, &binding).is_ok());
    // No kind has two secrets today, so `open` refuses another kind's field first; below it,
    // the field's name in the associated data keeps a value from opening as another field.
    let (header, payload) = sealed.as_str().rsplit_once(':').unwrap();
    let payload = base64_decode(payload);
    let decrypt = |field| {
        super::cipher(&key(1)).decrypt(super::associated_data(header, field, &binding), &payload)
    };
    assert!(decrypt(CALENDAR_ICS_URL).is_ok());
    assert!(decrypt(GITLAB_TOKEN).is_err());
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
        Some("ftp://gitlab.example.test"),
        Some("file:///etc/gitlab"),
        Some("https://jane@gitlab.example.test"),
        Some("https://jane:secret@gitlab.example.test"),
        Some("https://:secret@gitlab.example.test"),
    ] {
        let mut pairs = vec![("GITLAB_USER", "jane")];
        pairs.extend(url.map(|url| ("GITLAB_URL", url)));
        assert_eq!(
            Binding::of(&Source::Gitlab, &values(&pairs)),
            Err(BindingError::NoOrigin { key: "GITLAB_URL" }),
            "{url:?}"
        );
    }
}

/// Hosts are compared as `url` serializes them: IDNA as ASCII, IPv6 in brackets, the port
/// always written.
#[test]
fn a_github_token_is_sealed_for_the_web_origin() {
    let binding =
        |url: &str| Binding::of(&Source::Github, &values(&[("GITHUB_URL", url)])).unwrap();
    let original = binding("https://github.com/acme/widgets");
    let sealed = seal(
        &key(1),
        StoreKind::Os,
        GITHUB_TOKEN,
        &original,
        "github-example-token",
    )
    .unwrap();

    let same_origin = binding("https://github.com:443/settings");
    assert!(open(&sealed, &key(1), GITHUB_TOKEN, &same_origin).is_ok());
    for url in [
        "https://api.github.com",
        "https://acme.ghe.com",
        "https://github.com:8443",
    ] {
        assert!(matches!(
            open(&sealed, &key(1), GITHUB_TOKEN, &binding(url)),
            Err(Unopenable::OtherAddress)
        ));
    }
}

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
        &Source::Gitlab,
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

/// Base64 is read canonically: a last character whose unused bits are set is refused, so one
/// payload has one text.
#[test]
fn a_payload_with_set_tail_bits_is_malformed() {
    let good = sealed_token(&gitlab("https://gitlab.example.test"));
    let (head, payload) = good.as_str().rsplit_once(':').unwrap();
    // 31 bytes in 42 characters: the last one carries 2 bits of data and 4 unused ones.
    assert_eq!(base64_decode(payload).len(), 31);
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let last = payload.as_bytes()[payload.len() - 1];
    let index = ALPHABET.iter().position(|&c| c == last).unwrap();
    assert_eq!(index & 0b1111, 0, "a canonical tail");
    let tampered = ALPHABET[index | 0b0001] as char;
    let text = format!("{head}:{}{tampered}", &payload[..payload.len() - 1]);
    assert_eq!(SealedValue::parse(&text).err(), Some(Malformed::Payload));
}

/// RFC 5297's own vectors through the same multi-header AES-SIV API (`aes_siv::siv`): its
/// examples use a 256-bit key, so they run through `Aes128Siv`, the same generic S2V and CTR
/// code as our `Aes256Siv`, whose output the pinned vectors and OpenSSL check. A crate bump
/// that changed either would fail here.
#[test]
fn rfc_5297_vectors_hold() {
    use aes_siv::{KeyInit, siv::Aes128Siv};
    let hex = |text: &str| hex::decode(text).unwrap();

    // A.1, deterministic.
    let mut a1 = Aes128Siv::new_from_slice(&hex(
        "fffefdfcfbfaf9f8f7f6f5f4f3f2f1f0f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff",
    ))
    .unwrap();
    let headers = [hex("101112131415161718191a1b1c1d1e1f2021222324252627")];
    let sealed = a1
        .encrypt(&headers, &hex("112233445566778899aabbccddee"))
        .unwrap();
    assert_eq!(
        sealed,
        hex("85632d07c6e8f37f950acd320a2ecc9340c02b9690c4dc04daef7f6afe5c")
    );
    assert_eq!(
        a1.decrypt(&headers, &sealed).unwrap(),
        hex("112233445566778899aabbccddee")
    );

    // A.2, with a nonce as the last header.
    let mut a2 = Aes128Siv::new_from_slice(&hex(
        "7f7e7d7c7b7a79787776757473727170404142434445464748494a4b4c4d4e4f",
    ))
    .unwrap();
    let headers = [
        hex("00112233445566778899aabbccddeeffdeaddadadeaddadaffeeddccbbaa99887766554433221100"),
        hex("102030405060708090a0"),
        hex("09f911029d74e35bd84156c5635688c0"),
    ];
    let plaintext = hex(
        "7468697320697320736f6d6520706c61696e7465787420746f20656e6372797074207573696e67205349562d414553",
    );
    let sealed = a2.encrypt(&headers, &plaintext).unwrap();
    assert_eq!(
        sealed,
        hex(
            "7bdb6e3b432667eb06f4d14bff2fbd0fcb900f2fddbe404326601965c889bf17dba77ceb094fa663b7a3f748ba8af829ea64ad544a272e9c485b62a3fd5c0d"
        )
    );
    assert_eq!(a2.decrypt(&headers, &sealed).unwrap(), plaintext);
    let mut tampered = sealed.clone();
    tampered[20] ^= 1;
    assert!(a2.decrypt(&headers, &tampered).is_err());
}

#[test]
fn a_figma_token_opens_after_the_file_list_changes() {
    let binding = Binding::of(&Source::Figma, &values(&[])).unwrap();
    let sealed = seal(
        &key(1),
        StoreKind::File,
        FIGMA_TOKEN,
        &binding,
        "test-figma-token",
    )
    .unwrap();

    let with_files = Binding::of(
        &Source::Figma,
        &values(&[("FIGMA_FILES", r#"["F1","F2"]"#)]),
    )
    .unwrap();
    let opened = open(&sealed, &key(1), FIGMA_TOKEN, &with_files).unwrap();
    assert_eq!(opened.as_str(), "test-figma-token");
}
