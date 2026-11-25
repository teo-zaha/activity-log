use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::provider::remote_error::{
    ApiError, QueryAnswer, QueryErrorKind, QueryPathSegment, QueryRefusal,
};

/// Read a paged answer after discarding only refused slots in its expected lists.
pub(super) fn read<T: DeserializeOwned>(
    answer: QueryAnswer,
    lists: &[&[&str]],
    skippable: &[QueryErrorKind],
) -> Result<(T, usize), ApiError> {
    let (data, refusal) = answer.into_parts();
    let Some(mut data) = data else {
        return Err(refusal.map_or(ApiError::Decode, ApiError::Query));
    };
    let count = if let Some(refusal) = &refusal {
        remove_refused_rows(&mut data, refusal, lists, skippable)
            .ok_or_else(|| ApiError::Query(refusal.clone()))?
    } else {
        0
    };
    let decoded = serde_json::from_value(data)
        .map_err(|_| refusal.map_or(ApiError::Decode, ApiError::Query))?;
    Ok((decoded, count))
}

fn remove_refused_rows(
    data: &mut Value,
    refusal: &QueryRefusal,
    lists: &[&[&str]],
    skippable: &[QueryErrorKind],
) -> Option<usize> {
    let mut refused = vec![HashSet::new(); lists.len()];
    for error in refusal.errors() {
        if !skippable.contains(&error.kind) {
            return None;
        }
        let path = error.path.as_deref()?;
        let (list, row) = lists
            .iter()
            .enumerate()
            .find_map(|(list, fields)| row_index(path, fields).map(|row| (list, row)))?;
        refused[list].insert(row);
    }
    let mut count = 0;
    for (fields, refused) in lists.iter().zip(refused) {
        if refused.is_empty() {
            continue;
        }
        count += remove_rows(data, fields, &refused)?;
    }
    Some(count)
}

pub(super) fn remove_rows(
    data: &mut Value,
    fields: &[&str],
    refused: &HashSet<usize>,
) -> Option<usize> {
    let rows = fields
        .iter()
        .try_fold(data, |value, &field| value.get_mut(field))?
        .as_array_mut()?;
    if refused.iter().any(|&index| index >= rows.len()) {
        return None;
    }
    let mut index = 0;
    rows.retain(|_| {
        let keep = !refused.contains(&index);
        index += 1;
        keep
    });
    Some(refused.len())
}

pub(super) fn row_index(path: &[QueryPathSegment], fields: &[&str]) -> Option<usize> {
    use QueryPathSegment::{Field, Index};

    let [Index(index), ..] = path.get(fields.len()..)? else {
        return None;
    };
    fields
        .iter()
        .zip(path)
        .all(|(&name, part)| matches!(part, Field(field) if field == name))
        .then_some(*index)
}
