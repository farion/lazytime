use chrono::Datelike;
use lazytime::config::Config;
use lazytime::db;
use lazytime::tui::trackings_cleanup::cleanup_today_unsynced_trackings;
use std::collections::BTreeMap;
use tempfile::tempdir;

fn test_config(
    db_path: &std::path::Path,
    working_hours: BTreeMap<u8, Vec<lazytime::config::TimeRange>>,
) -> Config {
    Config {
        default_project: "DefaultProject".to_string(),
        tracking_stability_seconds: 10,
        working_hours,
        track_reminder_seconds: 300,
        track_reminder_snooze_seconds: 1800,
        summary_update_seconds: 5,
        report_start: None,
        report_end: None,
        db_file: db_path.to_string_lossy().to_string(),
        jira_url: None,
        jira_token: None,
        jira_email: None,
        jira_project: None,
        jira_assignee: None,
        jira_issue_type: "Story".to_string(),
        jira_sap_field: "sap_project".to_string(),
        ipc_socket_path: None,
    }
}

#[test]
fn cleanup_merges_same_project_unsynced_without_gap() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");

    let today = chrono::Local::now().date_naive();
    let ts = |h: u32, m: u32| format!("{}T{:02}:{:02}:00", today.format("%Y-%m-%d"), h, m);

    db::add_manual_tracking(&conn, "A", &ts(9, 0), Some(&ts(9, 30)), None).expect("insert 1");
    db::add_manual_tracking(&conn, "A", &ts(9, 30), Some(&ts(10, 0)), None).expect("insert 2");

    let config = test_config(&db_path, BTreeMap::new());

    let stats = cleanup_today_unsynced_trackings(&conn, &config).expect("cleanup");
    assert_eq!(stats.merged_groups, 1);
    assert_eq!(stats.removed_rows, 1);

    let rows = db::list_today(&conn).expect("list today");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].project_name, "A");
    assert_eq!(rows[0].start_ts, ts(9, 0));
    let expected_end = ts(10, 0);
    assert_eq!(rows[0].end_ts.as_deref(), Some(expected_end.as_str()));
}

#[test]
fn cleanup_keeps_rows_when_gap_exists() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");

    let today = chrono::Local::now().date_naive();
    let ts = |h: u32, m: u32| format!("{}T{:02}:{:02}:00", today.format("%Y-%m-%d"), h, m);

    db::add_manual_tracking(&conn, "A", &ts(9, 0), Some(&ts(9, 30)), None).expect("insert 1");
    db::add_manual_tracking(&conn, "A", &ts(9, 32), Some(&ts(10, 0)), None).expect("insert 2");

    let mut working_hours = BTreeMap::new();
    let weekday = chrono::Local::now().weekday().num_days_from_monday() as u8;
    working_hours.insert(
        weekday,
        vec![lazytime::config::TimeRange {
            start: "08:00".to_string(),
            end: "18:00".to_string(),
        }],
    );

    let config = test_config(&db_path, working_hours);

    let stats = cleanup_today_unsynced_trackings(&conn, &config).expect("cleanup");
    assert_eq!(stats.merged_groups, 0);
    assert_eq!(stats.removed_rows, 0);

    let rows = db::list_today(&conn).expect("list today");
    assert_eq!(rows.len(), 2);
}

#[test]
fn cleanup_does_not_merge_synced_rows() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");

    let today = chrono::Local::now().date_naive();
    let ts = |h: u32, m: u32| format!("{}T{:02}:{:02}:00", today.format("%Y-%m-%d"), h, m);

    db::add_manual_tracking(&conn, "A", &ts(9, 0), Some(&ts(9, 30)), None).expect("insert 1");
    db::add_manual_tracking(&conn, "A", &ts(9, 30), Some(&ts(10, 0)), None).expect("insert 2");

    let rows = db::list_today(&conn).expect("rows");
    let first_id = rows.first().expect("first row").id;
    db::set_tracking_synced(&conn, first_id, 1).expect("mark synced");

    let config = test_config(&db_path, BTreeMap::new());

    let stats = cleanup_today_unsynced_trackings(&conn, &config).expect("cleanup");
    assert_eq!(stats.merged_groups, 0);
    assert_eq!(stats.removed_rows, 0);

    let rows = db::list_today(&conn).expect("list today");
    assert_eq!(rows.len(), 2);
}
