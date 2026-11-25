use serde::Deserialize;
use serde_json::{Value, json};

use super::{Page, PageError, Pager};

#[derive(Deserialize)]
struct Row {
    id: String,
}

fn page(nodes: Vec<Value>, has_more: bool, after: Option<&str>) -> Page {
    serde_json::from_value(json!({
        "pageInfo": {"hasNextPage": has_more, "endCursor": after},
        "nodes": nodes,
    }))
    .unwrap()
}

#[tokio::test]
async fn pages_keep_fresh_rows_and_reject_a_repeated_position_before_fetching() {
    let mut pager = Pager::new();
    let first = pager
        .next::<_, PageError>(
            async |after| {
                assert_eq!(after, None);
                Ok(page(
                    vec![json!({"id":"one"}), Value::Null],
                    true,
                    Some("cursor"),
                ))
            },
            |row: &Row| &row.id,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.rows[0].id, "one");
    assert_eq!(first.skipped, 1);
    assert!(pager.has_next());

    let next = pager
        .next::<_, PageError>(
            async |after| {
                assert_eq!(after, Some("cursor"));
                Ok(page(
                    vec![json!({"id":"one"}), json!({"id":"two"})],
                    true,
                    after,
                ))
            },
            |row: &Row| &row.id,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.rows.len(), 1);
    assert_eq!(next.rows[0].id, "two");
    assert!(matches!(
        pager
            .next::<_, PageError>(
                async |_| panic!("repeated cursor was sent"),
                |row: &Row| &row.id
            )
            .await,
        Err(PageError::RepeatedPage)
    ));
}

#[tokio::test]
async fn more_pages_require_a_nonblank_cursor() {
    for cursor in [None, Some(" ")] {
        let result = Pager::new()
            .next::<_, PageError>(
                async |_| Ok(page(Vec::new(), true, cursor)),
                |row: &Row| &row.id,
            )
            .await;
        assert!(matches!(result, Err(PageError::MissingNextCursor)));
    }
}
