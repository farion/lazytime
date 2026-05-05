use lazytime::db;
use serde_json::json;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn report_outputs_aggregated_hours_for_range() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let cfg_path = dir.path().join("config.json");

    let conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");
    db::add_manual_tracking(
        &conn,
        "Alpha",
        "2026-04-29T08:00:00Z",
        Some("2026-04-29T09:30:00Z"),
        None,
    )
    .expect("alpha tracking");
    db::add_manual_tracking(
        &conn,
        "Beta",
        "2026-04-30T10:00:00Z",
        Some("2026-04-30T12:00:00Z"),
        None,
    )
    .expect("beta tracking");
    db::add_manual_tracking(&conn, "Open", "2026-04-30T13:00:00Z", None, None)
        .expect("open tracking");

    let cfg = json!({
        "default_project": "Default",
        "tracking_stability_seconds": 60,
        "working_hours": {},
        "track_reminder_seconds": 300,
        "track_reminder_snooze_seconds": 1800,
        "summary_update_seconds": 5,
        "db_file": db_path.to_string_lossy(),
        "jira_issue_type": "Story",
        "jira_email": null,
        "jira_sap_field": "sap_project"
    });
    fs::write(
        &cfg_path,
        serde_json::to_vec_pretty(&cfg).expect("cfg json"),
    )
    .expect("write cfg");

    let output = Command::new(env!("CARGO_BIN_EXE_lazytime"))
        .arg("--config")
        .arg(&cfg_path)
        .arg("--report")
        .arg("--start")
        .arg("2026-04-29")
        .arg("--end")
        .arg("2026-04-30")
        .output()
        .expect("run report");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2026-04-29 | Alpha | 1.50"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("2026-04-30 | Beta | 2.00"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("2026-04-30 | Open |"), "stdout={stdout}");
}

#[test]
fn summary_outputs_today_table() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let cfg_path = dir.path().join("config.json");

    let conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");
    db::add_manual_tracking(
        &conn,
        "Today",
        "2099-01-01T08:00:00Z",
        Some("2099-01-01T09:00:00Z"),
        None,
    )
    .expect("tracking");

    let cfg = json!({
        "default_project": "Default",
        "tracking_stability_seconds": 60,
        "working_hours": {},
        "track_reminder_seconds": 300,
        "track_reminder_snooze_seconds": 1800,
        "summary_update_seconds": 5,
        "db_file": db_path.to_string_lossy(),
        "jira_issue_type": "Story",
        "jira_email": null,
        "jira_sap_field": "sap_project"
    });
    fs::write(
        &cfg_path,
        serde_json::to_vec_pretty(&cfg).expect("cfg json"),
    )
    .expect("write cfg");

    let output = Command::new(env!("CARGO_BIN_EXE_lazytime"))
        .arg("--config")
        .arg(&cfg_path)
        .arg("--summary")
        .output()
        .expect("run summary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ID | Project | Start | End | Hours | Synced"));
}
