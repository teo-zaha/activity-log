use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use tokio::sync::OnceCell;

use crate::{
    diagnostics,
    model::{ConnectionId, ProviderProblem, Source},
    provider::{
        figma::{api::FigmaApi, scope::FigmaFileKey},
        remote_error::ApiError,
    },
};

use super::{file_book::FileBook, local_tabs};

/// One kind collection shares the tab scan. Each connection has its own token and book.
#[derive(Default)]
pub(crate) struct FigmaCollection {
    tabs: OnceCell<TabSnapshot>,
    runs: Mutex<HashMap<ConnectionId, Arc<ConnectionRun>>>,
}

impl FigmaCollection {
    pub(super) async fn tabs(&self) -> &TabSnapshot {
        self.tabs
            .get_or_init(|| async {
                let seen_at = Utc::now();
                let outcome = local_tabs::find().await;
                if let Some(warning) = outcome.warning() {
                    diagnostics::warn(Source::Figma.as_ref(), warning);
                }
                TabSnapshot { outcome, seen_at }
            })
            .await
    }

    pub(super) fn run(
        &self,
        connection: Option<&ConnectionId>,
        file_book: Option<&Path>,
    ) -> Arc<ConnectionRun> {
        let make = |scope| Arc::new(ConnectionRun::new(file_book.map(Path::to_owned), scope));
        let Some(id) = connection else {
            return make(Source::Figma.as_ref().to_owned());
        };
        let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        let scope = id.to_string();
        runs.entry(id.clone())
            .or_insert_with(|| make(scope))
            .clone()
    }
}

pub(super) struct TabSnapshot {
    pub(super) outcome: local_tabs::Outcome,
    pub(super) seen_at: DateTime<Utc>,
}

pub(super) struct ConnectionRun {
    scope: String,
    pub(super) book: Arc<FileBook>,
    pub(super) team_listing: OnceCell<()>,
    still_signed_in: OnceCell<Result<bool, ApiError>>,
    lost: Mutex<BTreeSet<FigmaFileKey>>,
}

impl ConnectionRun {
    fn new(path: Option<PathBuf>, scope: String) -> Self {
        Self {
            book: Arc::new(FileBook::new(path, scope.clone())),
            team_listing: OnceCell::new(),
            scope,
            still_signed_in: OnceCell::new(),
            lost: Mutex::default(),
        }
    }

    pub(super) async fn proves_loss(
        &self,
        api: &FigmaApi,
        principal: &str,
        error: &ApiError,
    ) -> Result<bool, ApiError> {
        match error {
            ApiError::Status {
                status: StatusCode::NOT_FOUND,
                ..
            } => Ok(true),
            // A confirmed token scope error leaves the file recorded and skips the identity probe.
            ApiError::Status {
                status: StatusCode::FORBIDDEN,
                ..
            } if error.problem() == Some(ProviderProblem::Refused) => self
                .still_signed_in
                .get_or_init(|| async {
                    api.me()
                        .await
                        .map(|answer| answer.is_some_and(|identity| identity.id == principal))
                })
                .await
                .clone(),
            _ => Ok(false),
        }
    }

    pub(super) fn record_lost(&self, lost: impl IntoIterator<Item = FigmaFileKey>) {
        self.lost
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(lost);
    }
}

impl Drop for ConnectionRun {
    fn drop(&mut self) {
        let count = self
            .lost
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        if count > 0 {
            diagnostics::warn(
                &self.scope,
                format!(
                    "Skipped Figma files that this connection could no longer access ({count} affected)."
                ),
            );
        }
    }
}
