use serde_json::json;

use super::{
    ChangeStack, change_stack_above::ChangeStackAbove, change_stack_bottom::ChangeStackBottom,
    change_stack_end::ChangeStackEnd, change_stack_node::ChangeStackNode, stack_cut::StackCut,
    stacked_change::StackedChange,
};

fn change(number: u64, title: &str) -> StackedChange {
    StackedChange {
        number,
        title: title.to_string(),
        web_url: Some(format!(
            "https://gitlab.example.org/acme/widgets/-/merge_requests/{number}"
        )),
    }
}

fn reached(branch: &str) -> ChangeStackBottom {
    ChangeStackBottom::Reached {
        branch: branch.to_string(),
    }
}

fn complete(changes: Vec<ChangeStackNode>) -> ChangeStackAbove {
    ChangeStackAbove {
        changes,
        end: ChangeStackEnd::Complete,
    }
}

fn node(change: StackedChange) -> ChangeStackNode {
    ChangeStackNode {
        change,
        above: complete(vec![]),
    }
}

#[test]
fn a_change_with_nothing_below_or_above_is_no_stack() {
    let lonely = ChangeStack::new(
        change(1, "Parse A"),
        vec![],
        reached("main"),
        complete(vec![]),
    );
    assert_eq!(lonely, None);
}

/// A stack whose ends were not all read says so; it is not "in no stack".
#[test]
fn a_change_whose_ends_were_cut_is_a_stack() {
    let cut = |reason| ChangeStackEnd::Cut { reason };
    for (bottom, end) in [
        (
            ChangeStackBottom::Cut {
                reason: StackCut::Limit,
            },
            ChangeStackEnd::Complete,
        ),
        (ChangeStackBottom::Ambiguous, ChangeStackEnd::Complete),
        (reached("main"), cut(StackCut::Throttled)),
    ] {
        let stack = ChangeStack::new(
            change(1, "Parse A"),
            vec![],
            bottom.clone(),
            ChangeStackAbove {
                changes: vec![],
                end,
            },
        );
        assert!(stack.is_some(), "{bottom:?} {end:?}");
    }
}

#[test]
fn a_change_below_or_above_makes_a_stack() {
    let below = ChangeStack::new(
        change(2, "Parse B"),
        vec![change(1, "Parse A")],
        reached("main"),
        complete(vec![]),
    )
    .unwrap();
    assert_eq!(below.below(), [change(1, "Parse A")]);
    assert_eq!(below.current(), &change(2, "Parse B"));
    assert_eq!(below.bottom(), &reached("main"));

    let above = ChangeStack::new(
        change(1, "Parse A"),
        vec![],
        reached("main"),
        complete(vec![node(change(2, "Parse B"))]),
    );
    assert!(above.is_some());
}

#[test]
fn blank_titles_and_branches_are_refused() {
    let base = || vec![change(1, "Parse A")];
    assert_eq!(
        ChangeStack::new(change(2, " "), base(), reached("main"), complete(vec![])),
        None
    );
    assert_eq!(
        ChangeStack::new(
            change(2, "Parse B"),
            base(),
            reached(" \t"),
            complete(vec![])
        ),
        None
    );
    assert_eq!(
        ChangeStack::new(
            change(2, "Parse B"),
            vec![change(1, "")],
            reached("main"),
            complete(vec![])
        ),
        None
    );
    let deep_blank = complete(vec![ChangeStackNode {
        change: change(3, "Parse C"),
        above: complete(vec![node(change(4, ""))]),
    }]);
    assert_eq!(
        ChangeStack::new(change(2, "Parse B"), base(), reached("main"), deep_blank),
        None
    );
}

#[test]
fn only_http_links_without_user_info_are_kept() {
    let with_link = |link: &str| StackedChange {
        web_url: Some(link.to_string()),
        ..change(2, "Parse B")
    };
    for refused in [
        "javascript:alert(1)",
        "data:text/html,hi",
        "https://jdoe:secret@gitlab.example.org/acme/widgets/-/merge_requests/2",
        "https://jdoe@gitlab.example.org/acme/widgets/-/merge_requests/2",
        "/acme/widgets/-/merge_requests/2",
        "file:///etc/passwd",
    ] {
        assert_eq!(
            ChangeStack::new(
                with_link(refused),
                vec![change(1, "Parse A")],
                reached("main"),
                complete(vec![])
            ),
            None,
            "{refused}"
        );
        let in_tree = complete(vec![node(with_link(refused))]);
        assert_eq!(
            ChangeStack::new(change(1, "Parse A"), vec![], reached("main"), in_tree),
            None,
            "{refused}"
        );
    }
    let without = StackedChange {
        web_url: None,
        ..change(2, "Parse B")
    };
    assert!(
        ChangeStack::new(
            without,
            vec![change(1, "Parse A")],
            reached("main"),
            complete(vec![])
        )
        .is_some()
    );
}

/// Reading a stack checks it as `new` does.
#[test]
fn a_stack_is_read_only_if_new_would_make_it() {
    let stack = |bottom: serde_json::Value, title: &str| {
        json!({
            "current": { "number": 2, "title": title },
            "below": [],
            "bottom": bottom,
            "above": { "changes": [], "end": { "kind": "complete" } },
        })
    };
    let read = |value: serde_json::Value| serde_json::from_value::<ChangeStack>(value);

    let ambiguous = read(stack(json!({ "kind": "ambiguous" }), "Parse B")).unwrap();
    assert_eq!(ambiguous.bottom(), &ChangeStackBottom::Ambiguous);
    assert_eq!(
        serde_json::to_value(&ambiguous).unwrap(),
        stack(json!({ "kind": "ambiguous" }), "Parse B")
    );

    assert!(
        read(stack(
            json!({ "kind": "reached", "branch": "main" }),
            "Parse B"
        ))
        .is_err()
    );
    assert!(read(stack(json!({ "kind": "ambiguous" }), "")).is_err());
    assert!(read(stack(json!({ "kind": "reached", "branch": "" }), "Parse B")).is_err());
    // `ambiguous` is no reason a level above ends for.
    let mut above_ambiguous = stack(json!({ "kind": "ambiguous" }), "Parse B");
    above_ambiguous["above"]["end"] = json!({ "kind": "cut", "reason": "ambiguous" });
    assert!(read(above_ambiguous).is_err());
}
