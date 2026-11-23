use activity_log_core::model::{LegacyFileDto, LegacyImportReportDto, LegacySource};

use super::ImportReport;

#[test]
fn the_report_is_taken_once() {
    let report = ImportReport::default();
    assert_eq!(report.take(), None);
    let imported = LegacyImportReportDto::Unreadable {
        file: LegacyFileDto {
            source: LegacySource::Settings,
            path: "/x/settings.json".to_string(),
        },
        message: "could not be read".to_string(),
    };
    report.hold(Some(imported.clone()));
    assert_eq!(report.take(), Some(imported));
    assert_eq!(report.take(), None);
}
