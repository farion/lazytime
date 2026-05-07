use lazytime::jira::JiraClient;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn create_issue_includes_custom_sap_field() {
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            captured.lock().expect("lock").push(req);

            let body = r#"{"key":"LT-123"}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = JiraClient::new(format!("http://{}", addr), "token".to_string(), None);
    let key = client
        .create_issue(
            "LT",
            Some("acc-123"),
            None,
            "summary",
            "description",
            "Task",
            "customfield_10001",
            "SAP-42",
        )
        .await
        .expect("create issue");
    assert_eq!(key, "LT-123");

    server.await.expect("server done");
    let reqs = requests.lock().expect("lock");
    let req = reqs.first().expect("request captured");
    let body = req.split("\r\n\r\n").nth(1).expect("body present");
    let payload: Value = serde_json::from_str(body).expect("json body");
    assert_eq!(payload["fields"]["issuetype"]["name"], "Task");
    assert_eq!(payload["fields"]["customfield_10001"], "SAP-42");
    assert_eq!(payload["fields"]["assignee"]["accountId"], "acc-123");
    assert_eq!(payload["fields"]["description"]["type"], "doc");
    assert_eq!(payload["fields"]["description"]["version"], 1);
}

#[test]
fn search_jql_uses_configured_field_and_current_user() {
    let jql = JiraClient::build_search_jql("LT", "customfield_22222", "SAP-7", None);
    assert!(jql.contains("customfield_22222 ~ \"SAP-7\""));
    assert!(jql.contains("assignee = currentUser()"));
}

#[test]
fn search_jql_quotes_human_readable_field() {
    let jql =
        JiraClient::build_search_jql("LT", "SAP-Nr-Projektaufgabe[Short text]", "SAP-7", None);
    assert!(jql.contains("\"SAP-Nr-Projektaufgabe[Short text]\" ~ \"SAP-7\""));
}

#[test]
fn search_jql_uses_configured_assignee_when_present() {
    let jql = JiraClient::build_search_jql("LT", "customfield_22222", "SAP-7", Some("alice"));
    assert!(jql.contains("assignee = \"alice\""));
}

#[tokio::test]
async fn add_worklog_uses_camel_case_payload() {
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            captured.lock().expect("lock").push(req);

            let body = r#"{"id":"987"}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = JiraClient::new(format!("http://{}", addr), "token".to_string(), None);
    let worklog_id = client
        .add_worklog("LT-7", "2026-05-04T10:00:00+02:00", 3600, "work")
        .await
        .expect("worklog");
    assert_eq!(worklog_id, "987");

    server.await.expect("server done");
    let reqs = requests.lock().expect("lock");
    let req = reqs.first().expect("request captured");
    let body = req.split("\r\n\r\n").nth(1).expect("body present");
    let payload: Value = serde_json::from_str(body).expect("json body");
    assert_eq!(payload["timeSpentSeconds"], 3600);
    assert_eq!(payload["started"], "2026-05-04T10:00:00.000+0200");
    assert_eq!(payload["comment"]["type"], "doc");
    assert_eq!(payload["comment"]["version"], 1);
}

#[tokio::test]
async fn add_worklog_rounds_up_to_minute() {
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            captured.lock().expect("lock").push(req);

            let body = r#"{"id":"111"}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = JiraClient::new(format!("http://{}", addr), "token".to_string(), None);
    let _ = client
        .add_worklog("LT-8", "2026-05-04T10:00:00+02:00", 3, "work")
        .await
        .expect("worklog");

    server.await.expect("server done");
    let reqs = requests.lock().expect("lock");
    let req = reqs.first().expect("request captured");
    let body = req.split("\r\n\r\n").nth(1).expect("body present");
    let payload: Value = serde_json::from_str(body).expect("json body");
    assert_eq!(payload["timeSpentSeconds"], 60);
}

#[tokio::test]
async fn issue_worklogs_parses_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf).await.expect("read");

            let body = r#"{"startAt":0,"maxResults":100,"total":1,"worklogs":[{"id":"w1","started":"2026-05-04T10:00:00.000+0200","timeSpentSeconds":3600,"author":{"accountId":"acc-123"},"comment":{"type":"doc","version":1,"content":[]}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = JiraClient::new(format!("http://{}", addr), "token".to_string(), None);
    let worklogs = client.issue_worklogs("LT-7").await.expect("issue worklogs");
    assert_eq!(worklogs.len(), 1);
    let wl = &worklogs[0];
    assert_eq!(wl.id, "w1");
    assert_eq!(wl.started, "2026-05-04T10:00:00.000+0200");
    assert_eq!(wl.time_spent_seconds, Some(3600));
    assert_eq!(
        wl.author.as_ref().and_then(|a| a.account_id.as_deref()),
        Some("acc-123")
    );

    server.await.expect("server done");
}

#[tokio::test]
async fn update_worklog_uses_put_payload() {
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            captured.lock().expect("lock").push(req);

            let body = r#"{"id":"123"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = JiraClient::new(format!("http://{}", addr), "token".to_string(), None);
    let worklog_id = client
        .update_worklog("LT-7", "123", "2026-05-04T10:00:00+02:00", 90, "work")
        .await
        .expect("update worklog");
    assert_eq!(worklog_id, "123");

    server.await.expect("server done");
    let reqs = requests.lock().expect("lock");
    let req = reqs.first().expect("request captured");
    assert!(req.starts_with("PUT /rest/api/3/issue/LT-7/worklog/123?adjustEstimate=auto "));
    let body = req.split("\r\n\r\n").nth(1).expect("body present");
    let payload: Value = serde_json::from_str(body).expect("json body");
    assert_eq!(payload["timeSpentSeconds"], 120);
    assert_eq!(payload["started"], "2026-05-04T10:00:00.000+0200");
    assert_eq!(payload["comment"]["type"], "doc");
}
