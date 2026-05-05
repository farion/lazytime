use lazytime::db;
use tempfile::tempdir;

#[test]
fn migration_creates_required_tables() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("test.sqlite");
    let conn = db::open(&db_path).expect("open db");
    db::migrate(&conn).expect("migrate");

    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type='table' AND name IN ('projects','project_rules','trackings','config_store')",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query");
    let mut names = Vec::new();
    for row in rows {
        names.push(row.expect("row"));
    }
    names.sort();
    assert_eq!(
        names,
        vec!["config_store", "project_rules", "projects", "trackings"]
    );
}
