use url::Url;

use crate::{
    model::{ProviderProblem, WebFixAction},
    provider::{
        connection::Connection,
        provider_config::{ConfigSource, ProviderConfigKey, value_problem},
        remote_api::public_base,
    },
};

pub struct CredentialSetup {
    pub(crate) token: ProviderConfigKey,
    pub(crate) address: CredentialAddress,
    pub(crate) primary: TokenPage,
    pub(crate) classic: Option<TokenPage>,
    pub(crate) instructions: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) enum CredentialAddress {
    Origin(ProviderConfigKey),
    Installation(ProviderConfigKey),
    Fixed(&'static str),
}

pub(crate) struct TokenPage {
    pub path: &'static [&'static str],
    pub parameters: &'static [(&'static str, &'static str)],
    pub permissions: &'static [TokenPermission],
    pub expiry_days: Option<u16>,
}

#[derive(Clone, Copy)]
pub(crate) enum TokenPermission {
    Manual(&'static str),
    Scope(&'static str),
    ReadOnly {
        name: &'static str,
        key: &'static str,
    },
}

impl TokenPermission {
    fn name(self) -> &'static str {
        match self {
            Self::Manual(name) | Self::Scope(name) | Self::ReadOnly { name, .. } => name,
        }
    }
}

impl TokenPage {
    pub(super) fn url(&self, mut base: Url) -> Option<String> {
        base.path_segments_mut()
            .ok()?
            .pop_if_empty()
            .extend(self.path);
        let mut parameters = self.parameters.to_vec();
        let mut scopes = Vec::new();
        for permission in self.permissions {
            match *permission {
                TokenPermission::Scope(scope) => scopes.push(scope),
                TokenPermission::ReadOnly { key, .. } => parameters.push((key, "read")),
                TokenPermission::Manual(_) => {}
            }
        }
        if self.expiry_days.is_some() || !parameters.is_empty() || !scopes.is_empty() {
            let mut query = base.query_pairs_mut();
            query.extend_pairs(parameters);
            if !scopes.is_empty() {
                query.append_pair("scopes", &scopes.join(","));
            }
            if let Some(days) = self.expiry_days {
                query.append_pair("expires_in", &days.to_string());
            }
        }
        Some(base.into())
    }
}

impl CredentialSetup {
    pub(crate) fn address_key(&self) -> Option<ProviderConfigKey> {
        match self.address {
            CredentialAddress::Origin(key) | CredentialAddress::Installation(key) => Some(key),
            CredentialAddress::Fixed(_) => None,
        }
    }

    pub(crate) fn saved(&self, connection: &Connection) -> Option<WebFixAction> {
        let address = self
            .address_key()
            .and_then(|key| connection.values().get(key.name));
        self.action(address.as_deref())
    }

    pub(crate) fn action(&self, address: Option<&str>) -> Option<WebFixAction> {
        let mut base = match self.address {
            CredentialAddress::Origin(key) | CredentialAddress::Installation(key) => {
                let address = address?;
                if value_problem(address, key).is_some() {
                    return None;
                }
                public_base(address).ok()?
            }
            CredentialAddress::Fixed(url) => public_base(url).ok()?,
        };
        if matches!(self.address, CredentialAddress::Origin(_)) {
            base.set_path("/");
        }
        let permissions = self.permission_names();
        let instructions = match permissions {
            Some(permissions) => format!("Include {permissions}. {}", self.instructions),
            None => self.instructions.to_owned(),
        };
        Some(WebFixAction::CreateToken {
            url: self.primary.url(base.clone())?,
            instructions,
            classic_url: self.classic.as_ref().and_then(|page| page.url(base)),
        })
    }

    pub(super) fn recovery_hint(&self, problem: ProviderProblem) -> String {
        match (problem, self.permission_names()) {
            (ProviderProblem::ScopeMissing, Some(permissions)) => {
                format!("The token needs {permissions}.")
            }
            (_, Some(permissions)) => {
                format!("Check that the token is active and grants {permissions}.")
            }
            _ => self.instructions.to_owned(),
        }
    }

    fn permission_names(&self) -> Option<String> {
        (!self.primary.permissions.is_empty()).then(|| {
            self.primary
                .permissions
                .iter()
                .map(|permission| permission.name())
                .collect::<Vec<_>>()
                .join(", ")
        })
    }
}
