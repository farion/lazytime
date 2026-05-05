use lazytime::db;
use tempfile::tempdir;

#[test]
fn mark_unsynced_clears_jira_refs() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let conn = db::open(&db_path).expect("open db");
    db::migrate(&conn).expect("migrate");

    db::add_manual_tracking(
        &conn,
        "Project A",
        "2026-05-05T08:00:00+00:00",
        Some("2026-05-05T09:00:00+00:00"),
        None,
    )
    .expect("add tracking");

    let rows = db::list_all_trackings(&conn).expect("list");
    let tracking_id = rows.first().expect("tracking row").id;

    db::mark_synced(&conn, tracking_id, "LT-42", "9001").expect("mark synced");
    let reference = db::jira_worklog_ref(&conn, tracking_id).expect("worklog ref");
    assert_eq!(reference, Some(("LT-42".to_string(), "9001".to_string())));

    db::mark_unsynced(&conn, tracking_id).expect("mark unsynced");
    let reference_after = db::jira_worklog_ref(&conn, tracking_id).expect("worklog ref after");
    assert_eq!(reference_after, None);

    let rows_after = db::list_all_trackings(&conn).expect("list after");
    let row = rows_after.first().expect("tracking row after");
    assert_eq!(row.jira_synced, 0);
}
