use super::{MARKER, Marker, Record, marker_beside};
use crate::model::{LegacyImportSkipDto, LegacySkipReason, LegacySource};

#[test]
fn a_written_record_reads_as_present_and_anything_else_counts_too() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    assert_eq!(marker_beside(&config), Marker::Absent);

    let record = Record {
        at: "2026-11-23T09:00:00+00:00".to_string(),
        used: LegacySource::Settings,
        path: "/x/settings.json".to_string(),
        found_unused: vec!["/y/.env".to_string()],
        imported: vec!["gitlab/gitlab".to_string(), "git/this-computer".to_string()],
        skipped: vec![LegacyImportSkipDto {
            connection: Some("gitlab/gitlab".parse().unwrap()),
            field: "GITLAB_TOKEN".to_string(),
            reason: LegacySkipReason::NoAddress,
        }],
    };
    let text = record.to_toml();
    let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(doc["release"].as_str(), Some("0.0.7"));
    assert_eq!(doc["used"].as_str(), Some("settings"));
    assert_eq!(doc["imported"].as_array().unwrap().len(), 2);
    assert_eq!(
        doc["skipped"][0]["reason"].as_str(),
        Some("noAddress"),
        "{text}"
    );
    std::fs::write(dir.path().join(MARKER), &text).unwrap();
    assert_eq!(marker_beside(&config), Marker::Present);

    std::fs::write(dir.path().join(MARKER), "not = [toml").unwrap();
    assert_eq!(marker_beside(&config), Marker::Unreadable);
    std::fs::write(dir.path().join(MARKER), "release = \"9.9.9\"\n").unwrap();
    assert_eq!(marker_beside(&config), Marker::Unreadable);
}
