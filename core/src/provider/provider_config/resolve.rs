use anyhow::Result;

use super::{ProviderConfig, ProviderConfigKey};
use crate::provider::provider_context::ProviderContext;

pub trait ProviderConfigResolve: Sized + Send + Sync + Clone + std::fmt::Debug + 'static {
    fn requirements() -> &'static [ProviderConfigKey];
    fn from_config(config: &ProviderConfig) -> Result<Self>;
}

#[derive(Clone, Copy)]
pub struct ConfigBinding {
    pub requirements: &'static [ProviderConfigKey],
    pub resolve: fn(&mut ProviderContext) -> Result<()>,
}

impl ConfigBinding {
    pub fn of<T: ProviderConfigResolve>() -> Self {
        fn resolve_typed<T: ProviderConfigResolve>(ctx: &mut ProviderContext) -> Result<()> {
            ctx.with_typed::<T>()
        }
        Self {
            requirements: T::requirements(),
            resolve: resolve_typed::<T>,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmptyConfig;

impl ProviderConfigResolve for EmptyConfig {
    fn requirements() -> &'static [ProviderConfigKey] {
        &[]
    }

    fn from_config(_config: &ProviderConfig) -> Result<Self> {
        Ok(Self)
    }
}

#[macro_export]
macro_rules! provider_config {
    (
        $vis:vis struct $name:ident {
            $(
                $field_vis:vis $field:ident => $key:ident $label:literal : $vtype:ident
                    $( = $default:literal )? $( [$account:ident] )?
            ),* $(,)?
        }
    ) => {
        $(
            pub const $key: $crate::provider::provider_config::ProviderConfigKey =
                $crate::provider::provider_config::ProviderConfigKey {
                    name: stringify!($key),
                    label: $label,
                    required: $crate::provider_config!(@required $vtype $( , $default )? ),
                    shape: $crate::provider::provider_config::ProviderConfigShape::Single(
                        $crate::provider_config!(@value_type $vtype),
                    ),
                    account: $crate::provider_config!(@account $( $account )?),
                };
        )*

        #[derive(Clone)]
        $vis struct $name {
            $( $field_vis $field: $crate::provider_config!(@field_type $vtype $( , $default )? ), )*
        }

        impl ::std::fmt::Debug for $name {
            // `Password` values are tokens and secret links: never printed.
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_struct(stringify!($name))
                    $( .field(stringify!($field), $crate::provider_config!(@debug self.$field, $vtype)) )*
                    .finish()
            }
        }

        impl $crate::provider::provider_config::ProviderConfigResolve for $name {
            fn requirements() -> &'static [$crate::provider::provider_config::ProviderConfigKey] {
                &[ $( $key ),* ]
            }

            fn from_config(
                config: &$crate::provider::provider_config::ProviderConfig,
            ) -> ::anyhow::Result<Self> {
                Ok(Self {
                    $( $field: $crate::provider_config!(@resolve config, $key, $vtype $( , $default )? ), )*
                })
            }
        }

        impl $name {
            pub fn binding() -> $crate::provider::provider_config::ConfigBinding {
                $crate::provider::provider_config::ConfigBinding::of::<Self>()
            }
        }
    };

    // ── Value type mapping ────────────────────────────────────────────────
    (@value_type String)   => { $crate::provider::provider_config::ProviderConfigValueType::String };
    (@value_type Url)      => { $crate::provider::provider_config::ProviderConfigValueType::Url };
    (@value_type Password) => { $crate::provider::provider_config::ProviderConfigValueType::Password };
    (@value_type Email)    => { $crate::provider::provider_config::ProviderConfigValueType::Email };
    (@value_type Int)      => { $crate::provider::provider_config::ProviderConfigValueType::Int };
    (@value_type Bool)     => { $crate::provider::provider_config::ProviderConfigValueType::Bool };

    // ── Account keys: `url => GITLAB_URL: Url [account]` ─────────────────
    (@account) => { false };
    (@account account) => { true };

    // ── Required inference ────────────────────────────────────────────────
    (@required Bool)                  => { false };
    (@required $vtype:ident , $default:literal) => { false };
    (@required $vtype:ident)          => { true };

    // ── Debug value ───────────────────────────────────────────────────────
    (@debug $value:expr, Password) => { &"<redacted>" };
    (@debug $value:expr, Url) => { &$crate::provider::provider_config::url_for_debug(&$value) };
    (@debug $value:expr, $vtype:ident) => { &$value };

    // ── Rust field type ───────────────────────────────────────────────────
    (@field_type Bool)                            => { bool };
    (@field_type Int)                             => { i64 };
    (@field_type Int , $default:literal)          => { i64 };
    (@field_type $vtype:ident , $default:literal) => { String };
    (@field_type $vtype:ident)                    => { String };

    // ── from_config resolution ────────────────────────────────────────────
    (@resolve $config:ident, $key:ident, Bool) => {
        $config.bool($key)
    };
    (@resolve $config:ident, $key:ident, Int) => {
        $config.require($key)?.parse::<i64>()?
    };
    (@resolve $config:ident, $key:ident, Int , $default:literal) => {
        $config.get($key).and_then(|s| s.parse::<i64>().ok()).unwrap_or($default)
    };
    (@resolve $config:ident, $key:ident, $vtype:ident , $default:literal) => {
        $config.get($key).unwrap_or($default).to_string()
    };
    (@resolve $config:ident, $key:ident, $vtype:ident) => {
        $config.require($key)?.to_string()
    };
}
