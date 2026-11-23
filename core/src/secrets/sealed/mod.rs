//! Sealed values, format v1: a secret as `config.toml` holds it, sealed with the config key
//! (`docs/adr/0006-secure-secrets.md`).
//!
//! The text is `v1:<store>:<kid>:<btag>:<payload>`: the store that holds the key (`os`,
//! `file`), the key's id, the tag of the values the secret is bound to, and the AES-SIV
//! (RFC 5297, `Aes256Siv`) synthetic IV and ciphertext in unpadded base64url. The associated
//! data is a vector, each part authenticated on its own by S2V: the format's name, the header
//! (`v1:<store>:<kid>:<btag>`), the field's wire name, and one `<WIRE_NAME>=<value>` per
//! binding key of the kind, in declaration order. Sealing is deterministic: the same key,
//! secret, field and binding give the same text.
//!
//! Opening decides before it decrypts: a key with another id is [`Unopenable::OtherKey`], a
//! binding with another tag [`Unopenable::OtherAddress`], and only then a failed open
//! [`Unopenable::Damaged`].

use std::fmt;

use aes_siv::{KeyInit, siv::Aes256Siv};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use super::{DataKey, StoreKind};
use crate::{
    model::Source,
    provider::{
        adapter,
        provider_config::{ConfigSource, ProviderConfigKey, ProviderConfigValueType, parse_bool},
    },
};

#[cfg(test)]
mod tests;

const VERSION: &str = "v1";
/// The first part of every value's associated data.
const FORMAT: &[u8] = b"activity-log/v1";
const KEY_ID_DOMAIN: &[u8] = b"activity-log/v1 key id";
const BINDING_DOMAIN: &[u8] = b"activity-log/v1 binding";
/// AES-SIV's synthetic IV, which comes first in the payload.
const IV_LEN: usize = 16;

/// Which key sealed a value: the first 4 bytes of SHA-256(`activity-log/v1 key id` ‖ key),
/// written as 8 hex digits. It says which key is needed, never the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyId([u8; 4]);

impl KeyId {
    pub fn of(key: &DataKey) -> Self {
        let hash = Sha256::new()
            .chain_update(KEY_ID_DOMAIN)
            .chain_update(key.bytes())
            .finalize();
        Self(first_four(&hash))
    }
}

/// What a value is bound to: the first 4 bytes of SHA-256(`activity-log/v1 binding` ‖ the
/// number of components ‖ each component, length-prefixed), as 8 hex digits. It hashes values
/// that are in the file in clear, and it is authenticated as part of the header.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingTag([u8; 4]);

/// The values a kind's secrets are bound to, normalized: for GitLab its server's origin, for
/// Jira its origin and its `insecure` setting (the `[binds]` keys); for Calendar nothing. It
/// belongs to its kind: only that kind's fields are sealed or opened with it.
#[derive(Clone, PartialEq, Eq)]
pub struct Binding {
    source: Source,
    /// `<WIRE_NAME>=<normalized value>`, in declaration order.
    components: Vec<String>,
}

/// Why a binding could not be read from the values: such a value can neither be sealed nor
/// opened, so the secret is never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingError {
    /// A URL binding key whose value is no `http` or `https` address with a host, or one that
    /// names a user or a password (unset included): the address rules the remote client
    /// keeps, so what a token is bound to is what it would be sent to.
    NoOrigin { key: &'static str },
    /// A `[binds]` mark on a key of a type nothing binds by.
    Unsupported { key: &'static str },
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOrigin { key } => write!(f, "{key} is no address a token can be bound to"),
            Self::Unsupported { key } => write!(f, "{key} cannot bind a secret"),
        }
    }
}

impl std::error::Error for BindingError {}

impl Binding {
    /// The binding of a connection of `source`: each `[binds]` key its kind declares, in
    /// declaration order, with its value in `values` normalized. A URL counts as its origin as
    /// `url` parses it (scheme and host lowercased, a host as ASCII, IPv6 in brackets, the port
    /// always written: `https://gitlab.example.com:443`); a bool as every reader reads it,
    /// unset or unreadable meaning `false`.
    pub fn of(source: &Source, values: &dyn ConfigSource) -> Result<Self, BindingError> {
        let components = adapter(source)
            .config_requirements()
            .iter()
            .filter(|key| key.binds)
            .map(|key| {
                let value = values.get(key.name);
                normalized(*key, value.as_deref()).map(|value| format!("{}={value}", key.name))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            source: source.clone(),
            components,
        })
    }

    /// Whether `field` is one of this binding's kind.
    fn holds(&self, field: ProviderConfigKey) -> bool {
        adapter(&self.source).config_requirements().contains(&field)
    }

    pub fn tag(&self) -> BindingTag {
        let mut hash = Sha256::new()
            .chain_update(BINDING_DOMAIN)
            .chain_update((self.components.len() as u64).to_be_bytes());
        for component in &self.components {
            hash.update((component.len() as u64).to_be_bytes());
            hash.update(component.as_bytes());
        }
        BindingTag(first_four(&hash.finalize()))
    }
}

impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding")
            .field("source", &self.source)
            .field("components", &self.components)
            .finish()
    }
}

fn normalized(key: ProviderConfigKey, value: Option<&str>) -> Result<String, BindingError> {
    match key.value_type() {
        ProviderConfigValueType::Url => value
            .and_then(origin)
            .ok_or(BindingError::NoOrigin { key: key.name }),
        ProviderConfigValueType::Bool => {
            Ok(value.and_then(parse_bool).unwrap_or(false).to_string())
        }
        ProviderConfigValueType::String
        | ProviderConfigValueType::Int
        | ProviderConfigValueType::Password
        | ProviderConfigValueType::Email => Err(BindingError::Unsupported { key: key.name }),
    }
}

/// `scheme://host:port`, the port always written; path, query and fragment left out. Only an
/// `http` or `https` address without a user name or password has one.
fn origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    let credentials = !url.username().is_empty() || url.password().is_some();
    if !matches!(url.scheme(), "http" | "https") || credentials {
        return None;
    }
    let host = url.host()?;
    let port = url.port_or_known_default()?;
    Some(format!("{}://{host}:{port}", url.scheme()))
}

/// A sealed secret as the file holds it. Equal when the text is; its `Debug` shows only the
/// store and the key id.
#[derive(Clone)]
pub struct SealedValue {
    text: String,
    store: StoreKind,
    key_id: KeyId,
    binding: BindingTag,
    /// The synthetic IV, then the ciphertext.
    payload: Vec<u8>,
}

/// Why a text is no sealed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Malformed {
    /// Not five parts separated by `:`.
    Shape,
    /// Another format than `v1`.
    Version,
    /// A store other than `os` or `file`.
    Store,
    /// A key id that is not 8 lowercase hex digits.
    KeyId,
    /// A binding tag that is not 8 lowercase hex digits.
    BindingTag,
    /// A payload that is not canonical unpadded base64url, or shorter than a synthetic IV.
    Payload,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shape => "it is not a sealed value",
            Self::Version => "it is sealed in a format this version does not read",
            Self::Store => "it names no store this version knows",
            Self::KeyId => "its key id is not 8 hex digits",
            Self::BindingTag => "its binding tag is not 8 hex digits",
            Self::Payload => "its sealed bytes are damaged",
        })
    }
}

impl std::error::Error for Malformed {}

/// Why a sealed value did not open, decided in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unopenable {
    /// Sealed with another key: on another computer, or before this one's store was reset.
    OtherKey,
    /// Sealed for another server or TLS setting than the file now names.
    OtherAddress,
    /// The key and the binding match, and still it does not open: edited or damaged.
    Damaged,
    /// The field is not one of the binding's kind: a caller's mistake, never a file's, and
    /// nothing is decrypted.
    OtherKind,
}

/// Why nothing was sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealError {
    /// The field holds no secret (`ProviderConfigKey::is_secret`): nothing else is sealed.
    NotASecret,
    /// An empty secret is no secret; a cleared field is unset.
    Empty,
    /// The field is not one of the binding's kind.
    OtherKind,
    /// The cipher refused (more associated data than AES-SIV takes).
    Cipher,
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotASecret => "only secrets are sealed",
            Self::Empty => "an empty secret is not sealed",
            Self::OtherKind => "the field is not one of the binding's kind",
            Self::Cipher => "the cipher refused the value",
        })
    }
}

impl std::error::Error for SealError {}

impl SealedValue {
    /// Reads `text`, strictly: one canonical form per value.
    pub fn parse(text: &str) -> Result<Self, Malformed> {
        let mut parts = text.split(':');
        let (Some(version), Some(store), Some(key_id), Some(binding), Some(payload), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(Malformed::Shape);
        };
        if version != VERSION {
            return Err(Malformed::Version);
        }
        let store = match store {
            "os" => StoreKind::Os,
            "file" => StoreKind::File,
            _ => return Err(Malformed::Store),
        };
        let key_id = KeyId(hex_four(key_id).ok_or(Malformed::KeyId)?);
        let binding = BindingTag(hex_four(binding).ok_or(Malformed::BindingTag)?);
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .ok()
            .filter(|payload| payload.len() >= IV_LEN)
            .ok_or(Malformed::Payload)?;
        Ok(Self {
            text: text.to_string(),
            store,
            key_id,
            binding,
            payload,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The store that holds the key this value needs.
    pub fn store(&self) -> StoreKind {
        self.store
    }

    pub fn key_id(&self) -> KeyId {
        self.key_id
    }

    /// Whether it was sealed for `binding`, read from the file alone, without any key.
    pub fn sealed_for(&self, binding: &Binding) -> bool {
        self.binding == binding.tag()
    }
}

impl PartialEq for SealedValue {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for SealedValue {}

impl fmt::Debug for SealedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedValue")
            .field("store", &self.store)
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

/// Seals `secret`, the value of `field`, with `key`, held in the store `store`, for `binding`.
pub fn seal(
    key: &DataKey,
    store: StoreKind,
    field: ProviderConfigKey,
    binding: &Binding,
    secret: &str,
) -> Result<SealedValue, SealError> {
    if !field.is_secret() {
        return Err(SealError::NotASecret);
    }
    if !binding.holds(field) {
        return Err(SealError::OtherKind);
    }
    if secret.is_empty() {
        return Err(SealError::Empty);
    }
    let key_id = KeyId::of(key);
    let tag = binding.tag();
    let header = header(store, key_id, tag);
    // Room for the IV up front, so the plaintext is never copied to a grown buffer; it is
    // overwritten by its ciphertext in place, and wiped if the cipher refuses.
    let mut payload = Zeroizing::new(Vec::with_capacity(secret.len() + IV_LEN));
    payload.extend_from_slice(secret.as_bytes());
    cipher(key)
        .encrypt_in_place(associated_data(&header, field, binding), &mut *payload)
        .map_err(|_| SealError::Cipher)?;
    let payload = std::mem::take(&mut *payload);
    let text = format!("{header}:{}", URL_SAFE_NO_PAD.encode(&payload));
    Ok(SealedValue {
        text,
        store,
        key_id,
        binding: tag,
        payload,
    })
}

/// Opens `value`, the value of `field`, with `key` (the key of the value's store), for the
/// binding the file now names.
pub fn open(
    value: &SealedValue,
    key: &DataKey,
    field: ProviderConfigKey,
    binding: &Binding,
) -> Result<Zeroizing<String>, Unopenable> {
    if !binding.holds(field) {
        return Err(Unopenable::OtherKind);
    }
    if KeyId::of(key) != value.key_id {
        return Err(Unopenable::OtherKey);
    }
    if !value.sealed_for(binding) {
        return Err(Unopenable::OtherAddress);
    }
    let header = header(value.store, value.key_id, value.binding);
    let mut buffer = Zeroizing::new(value.payload.clone());
    cipher(key)
        .decrypt_in_place(associated_data(&header, field, binding), &mut *buffer)
        .map_err(|_| Unopenable::Damaged)?;
    String::from_utf8(std::mem::take(&mut *buffer))
        .map(Zeroizing::new)
        .map_err(|err| {
            err.into_bytes().zeroize();
            Unopenable::Damaged
        })
}

fn cipher(key: &DataKey) -> Aes256Siv {
    Aes256Siv::new_from_slice(key.bytes()).expect("a config key is AES-256-SIV's 64 bytes")
}

fn header(store: StoreKind, key_id: KeyId, binding: BindingTag) -> String {
    format!("{VERSION}:{}:{key_id}:{binding}", store.as_ref())
}

fn associated_data<'a>(
    header: &'a str,
    field: ProviderConfigKey,
    binding: &'a Binding,
) -> Vec<&'a [u8]> {
    let mut parts = vec![FORMAT, header.as_bytes(), field.name.as_bytes()];
    parts.extend(binding.components.iter().map(String::as_bytes));
    parts
}

fn first_four(hash: &[u8]) -> [u8; 4] {
    [hash[0], hash[1], hash[2], hash[3]]
}

/// Exactly 8 lowercase hex digits.
fn hex_four(text: &str) -> Option<[u8; 4]> {
    if text.len() != 8 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut bytes = [0u8; 4];
    hex::decode_to_slice(text, &mut bytes).ok()?;
    Some(bytes)
}

macro_rules! hex_display {
    ($($type:ty),*) => {$(
        impl fmt::Display for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }

        impl fmt::Debug for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }
    )*};
}

hex_display!(KeyId, BindingTag);
