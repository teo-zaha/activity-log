use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_config_field_ref_dto::ProviderConfigFieldRefDto,
    provider_config_issue_kind_dto::ProviderConfigIssueKindDto,
};

/// A field of a settings save that would be refused, what is wrong with it, and what to do. The
/// message never repeats a value: it can be a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfigFieldIssueDto {
    pub field: ProviderConfigFieldRefDto,
    pub problem: ProviderConfigIssueKindDto,
    pub message: String,
}
