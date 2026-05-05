use lazytime::db;

#[test]
fn lock_acquire_and_release_behaves_as_expected() {
    let conn = rusqlite::Connection::open_in_memory().expect("open");
    db::migrate(&conn).expect("migrate");

    assert!(db::try_acquire_lock(&conn, "jira_sync_lock").expect("acquire first"));
    assert!(!db::try_acquire_lock(&conn, "jira_sync_lock").expect("acquire second"));

    db::release_lock(&conn, "jira_sync_lock").expect("release");
    assert!(db::try_acquire_lock(&conn, "jira_sync_lock").expect("acquire third"));
}
