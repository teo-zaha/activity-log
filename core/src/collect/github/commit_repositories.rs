use std::collections::{BTreeMap, HashSet};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::provider::remote_error::{
    ApiError, QueryAnswer, QueryErrorKind, QueryPathSegment, QueryRefusal,
};

use super::node_refusals;

pub(super) const REPOSITORIES_PER_REQUEST: usize = 20;

/// Root fields of queries with known repository ownership, and the number of IDs sent.
#[derive(Debug, Clone, Copy)]
pub(super) enum RepositoryQuery<'a> {
    Nodes { field: &'a str, count: usize },
    Node,
    Aliases(&'a [String]),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RepositoryNode<T> {
    Readable(T),
    Unavailable,
}

pub(super) struct RepositoryBatch<T> {
    pub(super) nodes: Vec<RepositoryNode<T>>,
    pub(super) refused_by_repository: Vec<usize>,
}

pub(super) struct RepositoryPair<T, U> {
    pub(super) primary: Vec<RepositoryNode<T>>,
    pub(super) secondary: Option<RepositoryBatch<U>>,
}

impl RepositoryQuery<'_> {
    /// All errors must identify an inaccessible requested repository or a field below it.
    /// Return one result per requested ID and decode only the unaffected nodes.
    pub(super) fn read<T: DeserializeOwned>(
        self,
        answer: QueryAnswer,
    ) -> Result<Vec<RepositoryNode<T>>, ApiError> {
        let (mut data, refusal) = answer.into_parts();
        self.read_rows(&mut data, refusal.as_ref(), None, None)
            .map(|batch| batch.nodes)
    }

    /// A refusal naming one row in a readable list leaves the other rows available.
    pub(super) fn read_with_row_skips<T: DeserializeOwned>(
        self,
        answer: QueryAnswer,
        list: &[&str],
    ) -> Result<RepositoryBatch<T>, ApiError> {
        let (mut data, refusal) = answer.into_parts();
        self.read_rows(&mut data, refusal.as_ref(), Some(list), None)
    }

    /// Two independently nullable roots in one answer. Errors must belong to exactly one.
    pub(super) fn read_pair<T: DeserializeOwned, U: DeserializeOwned>(
        self,
        other: Option<Self>,
        answer: QueryAnswer,
        other_list: &[&str],
    ) -> Result<RepositoryPair<T, U>, ApiError> {
        let (mut data, refusal) = answer.into_parts();
        let primary = self
            .read_rows(&mut data, refusal.as_ref(), None, other)?
            .nodes;
        let secondary = other
            .map(|query| query.read_rows(&mut data, refusal.as_ref(), Some(other_list), Some(self)))
            .transpose()?;
        Ok(RepositoryPair { primary, secondary })
    }

    fn read_rows<T: DeserializeOwned>(
        self,
        data: &mut Option<Value>,
        refusal: Option<&QueryRefusal>,
        list: Option<&[&str]>,
        other: Option<Self>,
    ) -> Result<RepositoryBatch<T>, ApiError> {
        let count = match self {
            Self::Nodes { count, .. } => count,
            Self::Node => 1,
            Self::Aliases(aliases) => aliases.len(),
        };
        let mut unavailable = vec![false; count];
        let mut refused_rows = BTreeMap::<usize, HashSet<usize>>::new();
        if let Some(refusal) = refusal {
            for error in refusal.errors() {
                if !matches!(
                    error.kind,
                    QueryErrorKind::Forbidden | QueryErrorKind::NotFound
                ) {
                    return Err(ApiError::Query(refusal.clone()));
                }
                let Some(path) = error.path.as_deref() else {
                    return Err(ApiError::Query(refusal.clone()));
                };
                let Some((index, path)) = self.at(path) else {
                    if other.is_some_and(|query| query.at(path).is_some()) {
                        continue;
                    }
                    return Err(ApiError::Query(refusal.clone()));
                };
                if let Some(row) = list.and_then(|fields| node_refusals::row_index(path, fields)) {
                    refused_rows.entry(index).or_default().insert(row);
                } else {
                    unavailable[index] = true;
                }
            }
            // A paired root can lose its data independently of the other root.
            let collapsed_root = match (self, other) {
                (Self::Nodes { field, .. }, Some(_)) => data
                    .as_ref()
                    .and_then(Value::as_object)
                    .is_some_and(|fields| fields.get(field).is_none_or(Value::is_null)),
                _ => false,
            };
            if unavailable.iter().all(|&marked| marked)
                && (collapsed_root || data.as_ref().is_none_or(Value::is_null))
            {
                return Ok(RepositoryBatch {
                    nodes: unavailable
                        .into_iter()
                        .map(|_| RepositoryNode::Unavailable)
                        .collect(),
                    refused_by_repository: vec![0; count],
                });
            }
        }
        let malformed = || refusal.cloned().map_or(ApiError::Decode, ApiError::Query);
        let rows = self.rows(data.as_mut().ok_or(ApiError::Decode)?)?;
        let mut refused_by_repository = vec![0; count];
        let repositories = rows
            .into_iter()
            .zip(unavailable)
            .enumerate()
            .map(|(index, (mut row, unavailable))| {
                if unavailable {
                    return Ok(RepositoryNode::Unavailable);
                }
                if let Some((fields, refused)) = list.zip(refused_rows.get(&index)) {
                    refused_by_repository[index] =
                        node_refusals::remove_rows(&mut row, fields, refused)
                            .ok_or_else(malformed)?;
                }
                if row.is_null() {
                    return Ok(RepositoryNode::Unavailable);
                }
                serde_json::from_value(row)
                    .map(RepositoryNode::Readable)
                    .map_err(|_| ApiError::Decode)
            })
            .collect::<Result<_, _>>()?;
        Ok(RepositoryBatch {
            nodes: repositories,
            refused_by_repository,
        })
    }

    fn at(self, path: &[QueryPathSegment]) -> Option<(usize, &[QueryPathSegment])> {
        use QueryPathSegment::{Field, Index};

        match (self, path) {
            (Self::Nodes { field, count }, [Field(root), Index(index), tail @ ..])
                if root == field && *index < count =>
            {
                Some((*index, tail))
            }
            (Self::Aliases(aliases), [Field(root), tail @ ..]) => aliases
                .iter()
                .position(|alias| alias == root)
                .map(|index| (index, tail)),
            (Self::Node, [Field(root), tail @ ..]) if root == "node" => Some((0, tail)),
            _ => None,
        }
    }

    fn rows(self, data: &mut Value) -> Result<Vec<Value>, ApiError> {
        let fields = data.as_object_mut().ok_or(ApiError::Decode)?;
        match self {
            Self::Node => fields
                .remove("node")
                .map(|row| vec![row])
                .ok_or(ApiError::Decode),
            Self::Nodes { field, count } => match fields.remove(field) {
                Some(Value::Array(rows)) if rows.len() == count => Ok(rows),
                _ => Err(ApiError::Decode),
            },
            Self::Aliases(aliases) => aliases
                .iter()
                .map(|alias| fields.remove(alias).ok_or(ApiError::Decode))
                .collect(),
        }
    }
}
