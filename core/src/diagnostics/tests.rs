use super::*;

#[test]
fn log_buffer_drops_the_oldest_entries_beyond_capacity() {
    let mut log = LogBuffer::new(2);
    log.push(LogLevel::Info, "a".into(), "first".into());
    log.push(LogLevel::Warn, "b".into(), "second".into());
    log.push(LogLevel::Error, "c".into(), "third".into());

    let entries = log.entries();
    let messages: Vec<_> = entries.iter().map(|entry| entry.message.as_str()).collect();
    assert_eq!(messages, ["second", "third"]);
    assert_eq!(entries[0].id, 2);
    assert_eq!(entries[1].id, 3);
}

#[test]
fn log_buffer_keeps_ids_increasing_after_clear() {
    let mut log = LogBuffer::new(4);
    log.push(LogLevel::Info, "a".into(), "before".into());
    log.clear();
    assert!(log.entries().is_empty());

    let id = log.push(LogLevel::Info, "a".into(), "after".into()).id;
    assert_eq!(id, 2);
}

#[test]
fn global_log_records_scope_level_and_message() {
    let marker = "global_log_records_scope_level_and_message";
    warn("tests", marker);

    let entry = entries()
        .into_iter()
        .find(|entry| entry.message == marker)
        .expect("entry recorded");
    assert_eq!(entry.level, LogLevel::Warn);
    assert_eq!(entry.scope, "tests");
}
