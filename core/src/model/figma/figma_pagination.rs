use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaPagination {
    pub(crate) next_page: Option<String>,
}
