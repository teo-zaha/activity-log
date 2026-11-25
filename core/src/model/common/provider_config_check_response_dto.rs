use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    provider_config_field_fix_dto::ProviderConfigFieldFixDto,
    provider_config_field_issue_dto::ProviderConfigFieldIssueDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigCheckResponseDto {
    pub issues: Vec<ProviderConfigFieldIssueDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub field_fix: Option<ProviderConfigFieldFixDto>,
}
