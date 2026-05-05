use lazytime::{db, rules};
use tempfile::tempdir;

#[tokio::test]
async fn invalid_regex_fails_loading() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let mut conn = db::open(&db_path).expect("open");
    db::migrate(&conn).expect("migrate");
    db::replace_rules(
        &mut conn,
        "ProjectA",
        Some("CP123"),
        &[("app", None, "(", 0)],
    )
    .expect("insert rules");

    let conn = db::open(&db_path).expect("open");
    let loaded = rules::load_rules(&conn);
    assert!(loaded.is_err());
}
