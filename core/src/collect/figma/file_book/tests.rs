use crate::model::Timestamp;

use super::*;

fn now() -> DateTime<Utc> {
    Timestamp::from("2026-06-01T12:00:00Z").utc()
}

fn key(name: &str) -> FigmaFileKey {
    FigmaFileKey::from_path(&format!("/design/{name}")).unwrap()
}

#[test]
fn merge_keeps_the_latest_of_each_fact_regardless_of_write_order() {
    let time = now();
    let old = time - TimeDelta::days(2);
    let mut a = Book::default();
    let mut b = Book::default();
    a.files.insert(
        key("Example"),
        FileRecord {
            last_tab: Some(time),
            last_reappearance: Some(old),
            ..FileRecord::default()
        },
    );
    b.files.insert(
        key("Another"),
        FileRecord {
            last_tab: Some(old),
            ..FileRecord::default()
        },
    );
    b.files.insert(
        key("Example"),
        FileRecord {
            last_tab: Some(old),
            last_contribution: Some(time),
            last_reappearance: Some(time),
            ..FileRecord::default()
        },
    );
    let mut ab = a.clone();
    ab.merge(b.clone());
    b.merge(a);
    assert_eq!(ab, b);
    assert_eq!(ab.files.len(), 2);
    let record = &ab.files[&key("Example")];
    assert_eq!(record.last_tab, Some(time));
    assert_eq!(record.last_contribution, Some(time));
    assert_eq!(record.last_reappearance, Some(time));
}

#[tokio::test]
async fn reloads_between_chunks_and_retains_files_still_contributing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design-0123456789abcdef.files.json");
    let first = Arc::new(FileBook::new(Some(path.clone()), "figma/design".into()));
    let second = Arc::new(FileBook::new(Some(path), "figma/design".into()));
    let time = now();
    let retention = TimeDelta::from_std(PRUNE_AFTER).unwrap();
    let then = time - retention;
    let discovered = first
        .begin_chunk(
            vec![key("Active"), key("Idle"), key("Pasted")],
            Some(then),
            BTreeSet::from([key("Pasted")]),
            then,
        )
        .await
        .unwrap();
    assert_eq!(discovered.read.len(), 2);
    let other_process = second
        .begin_chunk(Vec::new(), Some(then), BTreeSet::new(), then)
        .await
        .unwrap();
    assert_eq!(other_process.read.len(), 2);
    let active = other_process
        .read
        .into_iter()
        .find(|entry| entry.key == key("Active"))
        .unwrap();
    second
        .finish_chunk(
            vec![FileObservation {
                file: active,
                outcome: Observation::Read {
                    observed_at: time,
                    contributed: true,
                },
            }],
            time,
        )
        .await
        .unwrap();

    let later = time + TimeDelta::seconds(1);
    let retained = first
        .begin_chunk(Vec::new(), Some(later), BTreeSet::new(), later)
        .await
        .unwrap();
    assert_eq!(retained.read.len(), 1);
    assert_eq!(retained.read[0].key, key("Active"));
}

#[tokio::test]
async fn lost_files_have_one_retry_reservation_and_a_new_tab_brings_them_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design-0123456789abcdef.files.json");
    let first = Arc::new(FileBook::new(Some(path.clone()), "figma/design".into()));
    let second = Arc::new(FileBook::new(Some(path), "figma/design".into()));
    let time = now();
    let example = key("Example");
    let initial = first
        .begin_chunk(vec![example.clone()], Some(time), BTreeSet::new(), time)
        .await
        .unwrap();
    first
        .finish_chunk(
            vec![FileObservation {
                file: initial.read.into_iter().next().unwrap(),
                outcome: Observation::Lost { observed_at: time },
            }],
            time,
        )
        .await
        .unwrap();
    let still_open = time + TimeDelta::hours(3);
    let held = second
        .begin_chunk(
            vec![example.clone(), example.clone()],
            Some(still_open),
            BTreeSet::new(),
            still_open,
        )
        .await
        .unwrap();
    assert!(held.read.is_empty());
    let unreadable = time + TimeDelta::hours(4);
    second
        .begin_chunk(Vec::new(), None, BTreeSet::new(), unreadable)
        .await
        .unwrap();
    let still_open = time + TimeDelta::hours(5);
    let held = second
        .begin_chunk(
            vec![example.clone()],
            Some(still_open),
            BTreeSet::new(),
            still_open,
        )
        .await
        .unwrap();
    assert!(held.read.is_empty());

    let before = time + TimeDelta::hours(12);
    let held = second
        .begin_chunk(Vec::new(), Some(before), BTreeSet::new(), before)
        .await
        .unwrap();
    assert!(held.read.is_empty());
    assert_eq!(held.lost.as_slice(), std::slice::from_ref(&example));

    let next_day = time + TimeDelta::days(1);
    let (a, b) = tokio::join!(
        first.begin_chunk(Vec::new(), Some(next_day), BTreeSet::new(), next_day),
        second.begin_chunk(Vec::new(), Some(next_day), BTreeSet::new(), next_day),
    );
    assert_eq!(a.unwrap().read.len() + b.unwrap().read.len(), 1);

    let new_tab = next_day + TimeDelta::minutes(1);
    let reopened = second
        .begin_chunk(
            vec![example.clone()],
            Some(new_tab),
            BTreeSet::new(),
            new_tab,
        )
        .await
        .unwrap();
    assert_eq!(reopened.read[0].key, example);
}

#[tokio::test]
async fn leaves_invalid_and_newer_books_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design-0123456789abcdef.files.json");
    let disk = open_folder(temp.path()).unwrap();
    let name = path.file_name().unwrap().to_str().unwrap();
    let book = Arc::new(FileBook::new(Some(path.clone()), "figma/design".into()));
    for (contents, newer) in [
        (br#"{"version":2,"files":{}}"#.as_slice(), true),
        (b"malformed", false),
    ] {
        disk.replace(name, contents).unwrap();
        let result = book
            .begin_chunk(vec![key("Example")], Some(now()), BTreeSet::new(), now())
            .await;
        let Err(error) = result else {
            panic!("unreadable state should stop the chunk")
        };
        let preserved = match error.downcast_ref::<BookError>() {
            Some(BookError::Newer(path)) if newer => path,
            Some(BookError::Unreadable(path)) if !newer => path,
            other => panic!("unexpected book error: {other:?}"),
        };
        assert_eq!(preserved, &path);
        assert_eq!(std::fs::read(&path).unwrap(), contents);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_folder_other_accounts_can_change_uses_only_memory() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let temp = tempfile::tempdir().unwrap();
    let exposed = temp.path().join("cache");
    fs::create_dir(&exposed).unwrap();
    fs::set_permissions(&exposed, fs::Permissions::from_mode(0o777)).unwrap();
    let path = exposed.join("figma-design-0123456789abcdef.files.json");
    let book = Arc::new(FileBook::new(Some(path.clone()), "figma/design".into()));
    let time = now();
    let file = key("Example");
    book.begin_chunk(vec![file.clone()], Some(time), BTreeSet::new(), time)
        .await
        .unwrap();
    let chunk = book
        .begin_chunk(Vec::new(), Some(time), BTreeSet::new(), time)
        .await
        .unwrap();
    assert_eq!(chunk.read[0].key, file);
    assert!(!path.exists());
    assert_eq!(fs::read_dir(exposed).unwrap().count(), 0);
}

#[tokio::test]
async fn a_book_update_failure_keeps_the_path_and_cause_and_fails_the_chunk() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design-0123456789abcdef.files.json");
    let lock = path.with_extension("json.lock");
    std::fs::create_dir(&lock).unwrap();
    let book = Arc::new(FileBook::new(Some(path.clone()), "figma/design".into()));
    let result = book
        .begin_chunk(vec![key("Example")], Some(now()), BTreeSet::new(), now())
        .await;
    let Err(error) = result else {
        panic!("a failed book update should stop the chunk");
    };
    assert!(
        matches!(error.downcast_ref::<BookError>(), Some(BookError::Failed { path: failed, .. }) if failed == &path)
    );
    assert!(error.chain().count() > 1);
    assert!(!path.exists());
}
