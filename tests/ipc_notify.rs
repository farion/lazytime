use chrono::Utc;
use lazytime::db;
use lazytime::ipc::{client, server};
use lazytime::rules::{self, RuleCache, WindowEventInfo};
use lazytime::time;
use std::path::Path;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn ipc_projects_updated_roundtrip() {
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("lazytime.sock");
    let socket_str = socket_path.to_string_lossy().to_string();

    let (tx, mut rx) = mpsc::channel::<String>(8);
    let socket_for_server = socket_str.clone();
    let server_task = tokio::spawn(async move {
        let _ = server::run_ipc_server(&socket_for_server, tx).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    let ts = time::format_ts(&Utc::now());
    client::notify_projects_updated(&socket_str, &ts)
        .await
        .expect("notify");

    let received = tokio::time::timeout(tokio::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout")
        .expect("recv");
    assert_eq!(received, ts);

    server_task.abort();
}

#[tokio::test]
async fn ipc_reload_signal_swaps_rule_cache_for_detection() {
    let dir = tempdir().expect("tempdir");
    let socket_path = dir.path().join("lazytime.sock");
    let socket_str = socket_path.to_string_lossy().to_string();
    let db_path = dir.path().join("rules.sqlite");

    let mut conn = db::open(&db_path).expect("open db");
    db::migrate(&conn).expect("migrate");
    db::replace_rules(&mut conn, "Alpha", Some("CP1"), &[("app-a", None, ".*", 0)]).expect("alpha");

    let cache = RuleCache::default();
    let initial = rules::load_rules(&conn).expect("initial rules");
    cache.replace(initial).await;

    let (tx, mut rx) = mpsc::channel::<String>(8);
    let socket_for_server = socket_str.clone();
    let server_task = tokio::spawn(async move {
        let _ = server::run_ipc_server(&socket_for_server, tx).await;
    });

    let cache_for_reload = cache.clone();
    let db_path_string = db_path.to_string_lossy().to_string();
    let reload_task = tokio::spawn(async move {
        if rx.recv().await.is_some() {
            let conn = db::open(Path::new(&db_path_string)).expect("reload open");
            let loaded = rules::load_rules(&conn).expect("reload rules");
            cache_for_reload.replace(loaded).await;
        }
    });

    let before = cache.get().await.detect_project(&WindowEventInfo {
        app_id: Some("app-b".to_string()),
        instance: None,
        class: None,
        title: "x".to_string(),
    });
    assert!(before.is_none());

    db::replace_rules(&mut conn, "Beta", Some("CP2"), &[("app-b", None, ".*", 0)]).expect("beta");

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    client::notify_projects_updated(&socket_str, &time::format_ts(&Utc::now()))
        .await
        .expect("notify");

    tokio::time::timeout(tokio::time::Duration::from_secs(2), reload_task)
        .await
        .expect("reload timeout")
        .expect("reload join");

    let after = cache.get().await.detect_project(&WindowEventInfo {
        app_id: Some("app-b".to_string()),
        instance: None,
        class: None,
        title: "x".to_string(),
    });
    assert_eq!(after.as_deref(), Some("Beta"));

    server_task.abort();
}
