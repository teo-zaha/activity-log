use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, de::Error as _};

use crate::model::Timestamp;

use crate::provider::github::api::pager::Page;

#[derive(Deserialize)]
pub(super) struct Viewer<T> {
    pub(super) viewer: T,
}

#[derive(Deserialize)]
pub(super) struct User {
    #[serde(deserialize_with = "nonblank")]
    pub(super) login: String,
}

#[derive(Deserialize)]
pub(super) struct ViewerUser {
    #[serde(deserialize_with = "nonblank")]
    pub(super) id: String,
    #[serde(flatten)]
    pub(super) user: User,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Contributions {
    pub(super) contributions_collection: ContributionPage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContributionPage {
    pub(super) page: Page,
    pub(super) restricted_contributions_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Opened {
    pub(super) occurred_at: Timestamp,
    pub(super) item: HostedItem,
}

#[derive(Deserialize)]
pub(super) struct HostedItem {
    #[serde(deserialize_with = "nonblank")]
    pub(super) id: String,
    pub(super) number: NonZeroU64,
    #[serde(deserialize_with = "nonblank")]
    pub(super) title: String,
    pub(super) url: Option<String>,
    pub(super) repository: Repository,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Repository {
    #[serde(deserialize_with = "repository_name")]
    pub(super) name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReviewContribution {
    pub(super) pull_request: ReviewCandidate,
}

#[derive(Deserialize)]
pub(super) struct ReviewCandidate {
    #[serde(flatten)]
    pub(super) item: HostedItem,
    pub(super) reviews: Page,
}

#[derive(Deserialize)]
pub(super) struct OwnPullRequests {
    pub(super) page: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OwnPullRequest {
    pub(super) updated_at: Timestamp,
    #[serde(flatten)]
    pub(super) candidate: ReviewCandidate,
}

#[derive(Deserialize)]
pub(super) struct MoreReviews {
    pub(super) node: Reviews,
}

#[derive(Deserialize)]
pub(super) struct Reviews {
    pub(super) id: String,
    pub(super) reviews: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Review {
    #[serde(deserialize_with = "nonblank")]
    pub(super) id: String,
    pub(super) state: ReviewState,
    pub(super) submitted_at: Option<Timestamp>,
    pub(super) author: Option<User>,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

pub(super) fn nonblank<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.trim().is_empty() {
        return Err(D::Error::custom("expected nonblank text"));
    }
    Ok(value)
}

fn repository_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.split_once('/').is_some_and(|(owner, name)| {
        !owner.trim().is_empty() && !name.trim().is_empty() && !name.contains('/')
    }) {
        return Ok(value);
    }
    Err(D::Error::custom("expected a repository name with owner"))
}
