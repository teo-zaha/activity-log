use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::provider::remote_error::{ApiError, QueryAnswer, QueryErrorKind, QueryPathSegment};

/// The root fields of the two repository history queries and the number of IDs sent.
#[derive(Debug, Clone, Copy)]
pub(super) enum RepositoryQuery {
    Nodes(usize),
    Node,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RepositoryNode<T> {
    Readable(T),
    Unavailable,
}

impl RepositoryQuery {
    /// All errors must identify an inaccessible requested repository or a field below it.
    /// Return one result per requested ID and decode only the unaffected nodes.
    pub(super) fn read<T: DeserializeOwned>(
        self,
        answer: QueryAnswer,
    ) -> Result<Vec<RepositoryNode<T>>, ApiError> {
        let (data, refusal) = answer.into_parts();
        let count = match self {
            Self::Nodes(count) => count,
            Self::Node => 1,
        };
        let mut unavailable = vec![false; count];
        if let Some(refusal) = refusal {
            for error in refusal.errors() {
                let index = error.path.as_deref().and_then(|path| self.index_at(path));
                match (error.kind, index) {
                    (QueryErrorKind::Forbidden | QueryErrorKind::NotFound, Some(index)) => {
                        unavailable[index] = true;
                    }
                    _ => return Err(ApiError::Query(refusal)),
                }
            }
            // GraphQL can null the entire data object when no requested node is readable.
            if unavailable.iter().all(|&marked| marked) && data.as_ref().is_none_or(Value::is_null)
            {
                return Ok(unavailable
                    .into_iter()
                    .map(|_| RepositoryNode::Unavailable)
                    .collect());
            }
        }
        let rows = self.rows(data.ok_or(ApiError::Decode)?)?;
        rows.into_iter()
            .zip(unavailable)
            .map(|(row, unavailable)| {
                if unavailable || row.is_null() {
                    Ok(RepositoryNode::Unavailable)
                } else {
                    serde_json::from_value(row)
                        .map(RepositoryNode::Readable)
                        .map_err(|_| ApiError::Decode)
                }
            })
            .collect()
    }

    fn index_at(self, path: &[QueryPathSegment]) -> Option<usize> {
        use QueryPathSegment::{Field, Index};

        match (self, path) {
            (Self::Nodes(count), [Field(root), Index(index), ..])
                if root == "nodes" && *index < count =>
            {
                Some(*index)
            }
            (Self::Node, [Field(root), ..]) if root == "node" => Some(0),
            _ => None,
        }
    }

    fn rows(self, mut data: Value) -> Result<Vec<Value>, ApiError> {
        let fields = data.as_object_mut().ok_or(ApiError::Decode)?;
        match self {
            Self::Node => fields
                .remove("node")
                .map(|row| vec![row])
                .ok_or(ApiError::Decode),
            Self::Nodes(count) => match fields.remove("nodes") {
                Some(Value::Array(rows)) if rows.len() == count => Ok(rows),
                _ => Err(ApiError::Decode),
            },
        }
    }
}
