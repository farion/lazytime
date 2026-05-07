use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};

// When running inside the TUI we prefer to suppress jira client's tracing
// output to avoid printing below the TUI. This global flag lets jira_sync
// temporarily disable tracing emitted by this module.
static TRACING_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_tracing_enabled(enabled: bool) {
    TRACING_ENABLED.store(enabled, Ordering::SeqCst);
}

fn tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::SeqCst)
}

macro_rules! jira_debug {
    ($($arg:tt)*) => {
        if tracing_enabled() {
            tracing::debug!($($arg)*);
        }
    };
}

macro_rules! jira_info {
    ($($arg:tt)*) => {
        if tracing_enabled() {
            tracing::info!($($arg)*);
        }
    };
}

macro_rules! jira_warn {
    ($($arg:tt)*) => {
        if tracing_enabled() {
            tracing::warn!($($arg)*);
        }
    };
}

#[derive(Debug, Clone)]
pub struct JiraClient {
    base_url: String,
    token: String,
    email: Option<String>,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
struct SearchResponse {
    issues: Vec<IssueRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct IssueRef {
    key: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorklogResponse {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorklogAuthor {
    #[serde(rename = "accountId")]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorklogItem {
    pub id: String,
    pub started: String,
    #[serde(rename = "timeSpentSeconds")]
    pub time_spent_seconds: Option<i64>,
    pub comment: Option<Value>,
    pub author: Option<WorklogAuthor>,
}

#[derive(Debug, Clone, Deserialize)]
struct IssueWorklogsResponse {
    #[serde(default)]
    worklogs: Vec<WorklogItem>,
    #[serde(rename = "startAt")]
    start_at: Option<i64>,
    #[serde(rename = "maxResults")]
    max_results: Option<i64>,
    total: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct MyselfResponse {
    #[serde(rename = "accountId")]
    account_id: String,
}

impl JiraClient {
    pub fn new(base_url: String, token: String, email: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            email,
            client: reqwest::Client::new(),
        }
    }

    fn with_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(email) = &self.email {
            request.basic_auth(email, Some(&self.token))
        } else {
            request.bearer_auth(&self.token)
        }
    }

    fn is_retryable_status(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }

    async fn send_with_retry<F>(
        &self,
        make_request: F,
        operation: &str,
    ) -> Result<reqwest::Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let mut backoff = Duration::from_millis(500);
        let max_attempts = 3;
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 1..=max_attempts {
            match make_request().send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    if Self::is_retryable_status(status) && attempt < max_attempts {
                        jira_warn!(
                            "{} attempt {}/{} failed status={} - retrying in {:?}",
                            operation,
                            attempt,
                            max_attempts,
                            status,
                            backoff
                        );
                        sleep(backoff).await;
                        backoff *= 2;
                        continue;
                    }
                    return Err(anyhow!(
                        "{} failed with status {} body: {}",
                        operation,
                        status,
                        body
                    ));
                }
                Err(err) => {
                    if attempt < max_attempts {
                        jira_warn!(
                            "{} attempt {}/{} failed error={} - retrying in {:?}",
                            operation,
                            attempt,
                            max_attempts,
                            err,
                            backoff
                        );
                        last_error = Some(anyhow!(err));
                        sleep(backoff).await;
                        backoff *= 2;
                        continue;
                    }
                    return Err(anyhow!("{} request failed: {}", operation, err));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("{} failed after retries", operation)))
    }

    fn short_response_info(value: &Value) -> String {
        if let Some(key) = value.get("key").and_then(|v| v.as_str()) {
            return format!("key={}", key);
        }
        if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
            return format!("id={}", id);
        }
        if let Some(issues) = value.get("issues").and_then(|v| v.as_array()) {
            return format!("issues={}", issues.len());
        }
        if let Some(arr) = value.as_array() {
            return format!("items={}", arr.len());
        }
        if let Some(obj) = value.as_object() {
            return format!("fields={}", obj.len());
        }
        let text = value.to_string();
        if text.len() > 120 {
            format!("{}...", &text[..120])
        } else {
            text
        }
    }

    fn is_estimate_adjustment_error(message: &str) -> bool {
        let lower = message.to_ascii_lowercase();
        message.contains("\"timeSpent\"")
            || lower.contains("manual")
            || lower.contains("schätzungsanpassung")
    }

    // Read full response body, emit DEBUG payload log and INFO short request/response summary.
    // On parse/shape error we return an Err including the raw payload.
    async fn parse_response_json<T>(
        &self,
        response: reqwest::Response,
        operation: &str,
        request_short: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        jira_debug!(operation = %operation, status = %status, body = %text, "jira response body");
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                return Err(anyhow!("{} parse failed: {} body: {}", operation, e, text));
            }
        };
        let response_short = Self::short_response_info(&value);
        jira_info!(
            operation = %operation,
            request = %request_short,
            status = %status,
            response = %response_short,
            "jira request"
        );
        match serde_json::from_value::<T>(value) {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow!(
                "{} shape parse failed: {} body: {}",
                operation,
                e,
                text
            )),
        }
    }

    pub async fn authenticated_account_id(&self) -> Result<String> {
        let url = format!("{}/rest/api/3/myself", self.base_url);
        jira_debug!(url = %url, "jira request");
        let response = self
            .send_with_retry(
                || self.with_auth(self.client.get(&url)),
                "jira authenticated user lookup",
            )
            .await?;
        let body: MyselfResponse = self
            .parse_response_json(response, "jira authenticated user", "GET /myself")
            .await
            .context("jira authenticated user parse failed")?;
        Ok(body.account_id)
    }

    pub async fn find_issue(
        &self,
        jira_project: &str,
        sap_number: &str,
        sap_field_key: &str,
        assignee: Option<&str>,
    ) -> Result<Option<String>> {
        let jql = Self::build_search_jql(jira_project, sap_field_key, sap_number, assignee);
        let payload = json!({
            "jql": jql,
            "maxResults": 1
        });

        let url = format!("{}/rest/api/3/search/jql", self.base_url);
        // Also emit a ready-to-run curl command for debugging/replication.
        // Uses $JIRA_TOKEN placeholder so secrets are not leaked.
        let curl = self.curl_for_search(jira_project, sap_field_key, sap_number, assignee);
        jira_debug!(curl = %curl, "jira request curl");
        let response = self
            .send_with_retry(
                || self.with_auth(self.client.post(&url).json(&payload)),
                "jira search",
            )
            .await?;

        let request_short = format!(
            "POST /search/jql project={} sap_field={} sap={} assignee={}",
            jira_project,
            sap_field_key,
            sap_number,
            assignee.unwrap_or("currentUser()")
        );
        let body: SearchResponse = self
            .parse_response_json(response, "jira search", &request_short)
            .await
            .context("jira search parse failed")?;
        Ok(body
            .issues
            .first()
            .and_then(|i| i.key.clone().or_else(|| i.id.clone())))
    }

    pub async fn create_issue(
        &self,
        jira_project: &str,
        assignee_account_id: Option<&str>,
        assignee_name: Option<&str>,
        summary: &str,
        description: &str,
        issue_type: &str,
        sap_field_key: &str,
        sap_number: &str,
    ) -> Result<String> {
        #[derive(Deserialize)]
        struct CreateIssueResponse {
            key: String,
        }

        let mut fields = Map::<String, Value>::new();
        fields.insert("project".to_string(), json!({ "key": jira_project }));
        fields.insert("summary".to_string(), Value::String(summary.to_string()));
        fields.insert("description".to_string(), Self::adf_text(description));
        fields.insert("issuetype".to_string(), json!({ "name": issue_type }));
        if let Some(account_id) = assignee_account_id {
            fields.insert("assignee".to_string(), json!({ "accountId": account_id }));
        } else if let Some(name) = assignee_name {
            fields.insert("assignee".to_string(), json!({ "name": name }));
        }
        fields.insert(
            sap_field_key.to_string(),
            Value::String(sap_number.to_string()),
        );

        let payload = json!({ "fields": Value::Object(fields) });

        let url = format!("{}/rest/api/3/issue", self.base_url);
        let curl = self.curl_for_create_issue(
            jira_project,
            assignee_account_id,
            assignee_name,
            summary,
            description,
            issue_type,
            sap_field_key,
            sap_number,
        );
        jira_debug!(curl = %curl, "jira request curl");
        let response = self
            .send_with_retry(
                || self.with_auth(self.client.post(&url).json(&payload)),
                "jira create issue",
            )
            .await?;
        let request_short = format!(
            "POST /issue project={} type={} sap_field={} sap={}",
            jira_project, issue_type, sap_field_key, sap_number
        );
        let body: CreateIssueResponse = self
            .parse_response_json(response, "jira create issue", &request_short)
            .await
            .context("jira create issue parse failed")?;
        Ok(body.key)
    }

    /// Try to resolve a user identifier (email, name, or accountId) to an accountId.
    /// Returns Ok(Some(accountId)) on success, Ok(None) when not found or not resolvable,
    /// and Err on network/unexpected errors.
    pub async fn resolve_account_id(&self, identifier: &str) -> Result<Option<String>> {
        // First try user search (returns array)
        let url = format!("{}/rest/api/3/user/search", self.base_url);
        match self
            .send_with_retry(
                || self.with_auth(self.client.get(&url).query(&[("query", identifier)])),
                "jira user search",
            )
            .await
        {
            Ok(resp) => {
                let request_short = format!("GET /user/search query={}", identifier);
                let users: Vec<serde_json::Value> = match self
                    .parse_response_json(resp, "jira user search", &request_short)
                    .await
                {
                    Ok(v) => v,
                    Err(err) => {
                        jira_warn!("jira user search parse failed: {}", err);
                        Vec::new()
                    }
                };
                if let Some(first) = users.first() {
                    if let Some(account_id) = first.get("accountId").and_then(|v| v.as_str()) {
                        return Ok(Some(account_id.to_string()));
                    }
                }
            }
            Err(err) => {
                jira_warn!("jira user search failed: {}", err);
                // fallthrough to try accountId path
            }
        }

        // Try retrieving by accountId directly
        let url2 = format!("{}/rest/api/3/user", self.base_url);
        match self
            .send_with_retry(
                || self.with_auth(self.client.get(&url2).query(&[("accountId", identifier)])),
                "jira user by accountId",
            )
            .await
        {
            Ok(resp) => {
                let request_short = format!("GET /user accountId={}", identifier);
                let body: serde_json::Value = match self
                    .parse_response_json(resp, "jira user by accountId", &request_short)
                    .await
                {
                    Ok(v) => v,
                    Err(err) => {
                        jira_warn!("jira user by accountId parse failed: {}", err);
                        serde_json::Value::Null
                    }
                };
                if let Some(account_id) = body.get("accountId").and_then(|v| v.as_str()) {
                    return Ok(Some(account_id.to_string()));
                }
                Ok(None)
            }
            Err(err) => {
                jira_warn!("jira user by accountId failed: {}", err);
                Ok(None)
            }
        }
    }

    pub async fn add_worklog(
        &self,
        issue_key: &str,
        started: &str,
        seconds: i64,
        comment: &str,
    ) -> Result<String> {
        // Some Jira instances enforce minute granularity for logged time.
        // Round up to the nearest minute and provide timeSpentSeconds only.
        let rounded_seconds = if seconds <= 0 {
            60
        } else {
            ((seconds + 59) / 60) * 60
        };
        let rounded_minutes = rounded_seconds / 60;

        // Jira expects timestamps with milliseconds and timezone, like 2026-05-04T15:26:23.123+00:00
        // We accept RFC3339 input and convert to the expected format with milliseconds and offset.
        // Parse into a DateTime with fixed offset so we preserve the provided timezone
        let started_dt: chrono::DateTime<chrono::FixedOffset> = started
            .parse()
            .map_err(|e| anyhow!("invalid start timestamp: {}", e))?;
        // Jira expects timezone offset without colon: yyyy-MM-dd'T'HH:mm:ss.SSSZ
        // Example: 2026-05-04T10:00:00.000+0200
        let started_fmt = started_dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

        let payload = json!({
            "started": started_fmt,
            "timeSpentSeconds": rounded_seconds,
            "comment": Self::adf_text(comment),
        });
        let url = format!("{}/rest/api/3/issue/{}/worklog", self.base_url, issue_key);

        let curl_auto = self.curl_for_add_worklog(issue_key, started, seconds, comment);
        jira_debug!(curl = %curl_auto, "jira request curl");
        let response = match self
            .send_with_retry(
                || {
                    self.with_auth(
                        self.client
                            .post(&url)
                            .query(&[("adjustEstimate", "auto")])
                            .json(&payload),
                    )
                },
                "jira add worklog",
            )
            .await
        {
            Ok(resp) => resp,
            Err(err_auto) => {
                let msg_auto = err_auto.to_string();
                if !Self::is_estimate_adjustment_error(&msg_auto) {
                    return Err(err_auto);
                }

                jira_info!(
                    issue = %issue_key,
                    "jira add worklog: retrying with adjustEstimate=leave after estimate-adjustment error"
                );
                let curl_leave = self.curl_command(
                    "POST",
                    &format!("{}?adjustEstimate=leave", url),
                    Some(&payload),
                );
                jira_debug!(curl = %curl_leave, "jira request curl");
                match self
                    .send_with_retry(
                        || {
                            self.with_auth(
                                self.client
                                    .post(&url)
                                    .query(&[("adjustEstimate", "leave")])
                                    .json(&payload),
                            )
                        },
                        "jira add worklog (leave estimate)",
                    )
                    .await
                {
                    Ok(resp) => resp,
                    Err(err_leave) => {
                        let msg_leave = err_leave.to_string();
                        if !Self::is_estimate_adjustment_error(&msg_leave) {
                            return Err(err_leave);
                        }

                        let reduce_by = format!("{}m", rounded_minutes);
                        jira_info!(
                            issue = %issue_key,
                            reduce_by = %reduce_by,
                            "jira add worklog: retrying with adjustEstimate=manual and reduceBy"
                        );
                        let curl_manual = self.curl_command(
                            "POST",
                            &format!("{}?adjustEstimate=manual&reduceBy={}", url, reduce_by),
                            Some(&payload),
                        );
                        jira_debug!(curl = %curl_manual, "jira request curl");
                        self.send_with_retry(
                            || {
                                self.with_auth(
                                    self.client
                                        .post(&url)
                                        .query(&[
                                            ("adjustEstimate", "manual"),
                                            ("reduceBy", reduce_by.as_str()),
                                        ])
                                        .json(&payload),
                                )
                            },
                            "jira add worklog (manual estimate)",
                        )
                        .await?
                    }
                }
            }
        };
        let request_short = format!(
            "POST /issue/{}/worklog started={} seconds={} adjustEstimate=auto|leave|manual",
            issue_key, started, seconds
        );
        let body: WorklogResponse = self
            .parse_response_json(response, "jira worklog", &request_short)
            .await
            .context("jira worklog parse failed")?;
        Ok(body.id)
    }

    pub async fn issue_worklogs(&self, issue_key: &str) -> Result<Vec<WorklogItem>> {
        let mut start_at = 0i64;
        let mut out: Vec<WorklogItem> = Vec::new();

        loop {
            let url = format!("{}/rest/api/3/issue/{}/worklog", self.base_url, issue_key);
            let response = self
                .send_with_retry(
                    || {
                        self.with_auth(
                            self.client
                                .get(&url)
                                .query(&[("startAt", start_at), ("maxResults", 100i64)]),
                        )
                    },
                    "jira issue worklogs",
                )
                .await?;

            let request_short = format!(
                "GET /issue/{}/worklog startAt={} maxResults=100",
                issue_key, start_at
            );
            let body: IssueWorklogsResponse = self
                .parse_response_json(response, "jira issue worklogs", &request_short)
                .await
                .context("jira issue worklogs parse failed")?;

            let received = body.worklogs.len() as i64;
            out.extend(body.worklogs);

            let total = body.total.unwrap_or(out.len() as i64);
            let next_start = body.start_at.unwrap_or(start_at) + received;
            let max_results = body.max_results.unwrap_or(100);

            if received == 0 || out.len() as i64 >= total || received < max_results {
                break;
            }
            start_at = next_start;
        }

        Ok(out)
    }

    pub async fn update_worklog(
        &self,
        issue_key: &str,
        worklog_id: &str,
        started: &str,
        seconds: i64,
        comment: &str,
    ) -> Result<String> {
        let rounded_seconds = if seconds <= 0 {
            60
        } else {
            ((seconds + 59) / 60) * 60
        };
        let rounded_minutes = rounded_seconds / 60;

        let started_dt: chrono::DateTime<chrono::FixedOffset> = started
            .parse()
            .map_err(|e| anyhow!("invalid start timestamp: {}", e))?;
        let started_fmt = started_dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

        let payload = json!({
            "started": started_fmt,
            "timeSpentSeconds": rounded_seconds,
            "comment": Self::adf_text(comment),
        });
        let url = format!(
            "{}/rest/api/3/issue/{}/worklog/{}",
            self.base_url, issue_key, worklog_id
        );

        let curl_auto =
            self.curl_for_update_worklog(issue_key, worklog_id, started, seconds, comment);
        jira_debug!(curl = %curl_auto, "jira request curl");
        let response = match self
            .send_with_retry(
                || {
                    self.with_auth(
                        self.client
                            .put(&url)
                            .query(&[("adjustEstimate", "auto")])
                            .json(&payload),
                    )
                },
                "jira update worklog",
            )
            .await
        {
            Ok(resp) => resp,
            Err(err_auto) => {
                let msg_auto = err_auto.to_string();
                if !Self::is_estimate_adjustment_error(&msg_auto) {
                    return Err(err_auto);
                }

                jira_info!(
                    issue = %issue_key,
                    worklog = %worklog_id,
                    "jira update worklog: retrying with adjustEstimate=leave after estimate-adjustment error"
                );
                let curl_leave = self.curl_command(
                    "PUT",
                    &format!("{}?adjustEstimate=leave", url),
                    Some(&payload),
                );
                jira_debug!(curl = %curl_leave, "jira request curl");
                match self
                    .send_with_retry(
                        || {
                            self.with_auth(
                                self.client
                                    .put(&url)
                                    .query(&[("adjustEstimate", "leave")])
                                    .json(&payload),
                            )
                        },
                        "jira update worklog (leave estimate)",
                    )
                    .await
                {
                    Ok(resp) => resp,
                    Err(err_leave) => {
                        let msg_leave = err_leave.to_string();
                        if !Self::is_estimate_adjustment_error(&msg_leave) {
                            return Err(err_leave);
                        }

                        let reduce_by = format!("{}m", rounded_minutes);
                        jira_info!(
                            issue = %issue_key,
                            worklog = %worklog_id,
                            reduce_by = %reduce_by,
                            "jira update worklog: retrying with adjustEstimate=manual and reduceBy"
                        );
                        let curl_manual = self.curl_command(
                            "PUT",
                            &format!("{}?adjustEstimate=manual&reduceBy={}", url, reduce_by),
                            Some(&payload),
                        );
                        jira_debug!(curl = %curl_manual, "jira request curl");
                        self.send_with_retry(
                            || {
                                self.with_auth(
                                    self.client
                                        .put(&url)
                                        .query(&[
                                            ("adjustEstimate", "manual"),
                                            ("reduceBy", reduce_by.as_str()),
                                        ])
                                        .json(&payload),
                                )
                            },
                            "jira update worklog (manual estimate)",
                        )
                        .await?
                    }
                }
            }
        };

        let request_short = format!(
            "PUT /issue/{}/worklog/{} started={} seconds={} adjustEstimate=auto|leave|manual",
            issue_key, worklog_id, started, seconds
        );
        let body: WorklogResponse = self
            .parse_response_json(response, "jira update worklog", &request_short)
            .await
            .context("jira update worklog parse failed")?;
        Ok(body.id)
    }

    pub async fn delete_worklog(&self, issue_key: &str, worklog_id: &str) -> Result<()> {
        let url = format!(
            "{}/rest/api/3/issue/{}/worklog/{}",
            self.base_url, issue_key, worklog_id
        );
        jira_debug!(url = %url, "jira request");
        let _response = self
            .send_with_retry(
                || self.with_auth(self.client.delete(&url)),
                "jira delete worklog",
            )
            .await?;
        jira_info!(
            issue = %issue_key,
            worklog = %worklog_id,
            "jira worklog deleted"
        );
        Ok(())
    }

    fn adf_text(text: &str) -> Value {
        json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }
            ]
        })
    }

    // (removed) adf_text_pub helper — unused helper removed per request

    pub(crate) fn curl_command(&self, method: &str, url: &str, payload: Option<&Value>) -> String {
        // Build a curl command that uses an environment variable for the token to avoid leaking secrets.
        // Use double quotes around auth fragments so shells receive the username:token or header
        // with double quotes (e.g. -u "user:$JIRA_TOKEN"), matching expected logging format.
        let auth_part = if let Some(email) = &self.email {
            format!("-u \"{}:$JIRA_TOKEN\"", email)
        } else {
            "-H \"Authorization: Bearer $JIRA_TOKEN\"".to_string()
        };

        let mut cmd = format!(
            "curl -sS -X {} '{}' {} -H 'Content-Type: application/json'",
            method, url, auth_part
        );
        if let Some(p) = payload {
            if let Ok(s) = serde_json::to_string(p) {
                // Escape single quotes for embedding in a single-quoted shell string
                let escaped = s.replace("'", "'\"'\"'");
                cmd.push(' ');
                cmd.push_str(&format!("-d '{}'", escaped));
            }
        }
        cmd
    }

    /// Build a curl command for the search (POST /rest/api/3/search/jql)
    pub(crate) fn curl_for_search(
        &self,
        jira_project: &str,
        sap_field_key: &str,
        sap_number: &str,
        assignee: Option<&str>,
    ) -> String {
        let jql = Self::build_search_jql(jira_project, sap_field_key, sap_number, assignee);
        let payload = json!({ "jql": jql, "maxResults": 1 });
        let url = format!("{}/rest/api/3/search/jql", self.base_url);
        self.curl_command("POST", &url, Some(&payload))
    }

    /// Build a curl command for creating an issue (POST /rest/api/3/issue)
    pub(crate) fn curl_for_create_issue(
        &self,
        jira_project: &str,
        assignee_account_id: Option<&str>,
        assignee_name: Option<&str>,
        summary: &str,
        description: &str,
        issue_type: &str,
        sap_field_key: &str,
        sap_number: &str,
    ) -> String {
        let mut fields = Map::<String, Value>::new();
        fields.insert("project".to_string(), json!({ "key": jira_project }));
        fields.insert("summary".to_string(), Value::String(summary.to_string()));
        fields.insert("description".to_string(), Self::adf_text(description));
        fields.insert("issuetype".to_string(), json!({ "name": issue_type }));
        if let Some(account_id) = assignee_account_id {
            fields.insert("assignee".to_string(), json!({ "accountId": account_id }));
        } else if let Some(name) = assignee_name {
            fields.insert("assignee".to_string(), json!({ "name": name }));
        }
        fields.insert(
            sap_field_key.to_string(),
            Value::String(sap_number.to_string()),
        );
        let payload = json!({ "fields": Value::Object(fields) });
        let url = format!("{}/rest/api/3/issue", self.base_url);
        self.curl_command("POST", &url, Some(&payload))
    }

    /// Build a curl command for adding a worklog (POST /rest/api/3/issue/{issue}/worklog?adjustEstimate=auto)
    pub(crate) fn curl_for_add_worklog(
        &self,
        issue_key: &str,
        started: &str,
        seconds: i64,
        comment: &str,
    ) -> String {
        // Round as in add_worklog
        let rounded_seconds = if seconds <= 0 {
            60
        } else {
            ((seconds + 59) / 60) * 60
        };
        // normalize started similar to add_worklog: parse and format
        let started_dt: chrono::DateTime<chrono::FixedOffset> =
            started.parse().unwrap_or_else(|_| {
                // fallback: treat as UTC
                chrono::DateTime::parse_from_rfc3339(started).unwrap_or_else(|_| {
                    chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())
                })
            });
        let started_fmt = started_dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
        let payload = json!({
            "started": started_fmt,
            "timeSpentSeconds": rounded_seconds,
            "comment": Self::adf_text(comment),
        });
        let url = format!(
            "{}/rest/api/3/issue/{}/worklog?adjustEstimate=auto",
            self.base_url, issue_key
        );
        self.curl_command("POST", &url, Some(&payload))
    }

    /// Build a curl command for updating a worklog (PUT /rest/api/3/issue/{issue}/worklog/{id}?adjustEstimate=auto)
    pub(crate) fn curl_for_update_worklog(
        &self,
        issue_key: &str,
        worklog_id: &str,
        started: &str,
        seconds: i64,
        comment: &str,
    ) -> String {
        let rounded_seconds = if seconds <= 0 {
            60
        } else {
            ((seconds + 59) / 60) * 60
        };
        let started_dt: chrono::DateTime<chrono::FixedOffset> =
            started.parse().unwrap_or_else(|_| {
                chrono::DateTime::parse_from_rfc3339(started).unwrap_or_else(|_| {
                    chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())
                })
            });
        let started_fmt = started_dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
        let payload = json!({
            "started": started_fmt,
            "timeSpentSeconds": rounded_seconds,
            "comment": Self::adf_text(comment),
        });
        let url = format!(
            "{}/rest/api/3/issue/{}/worklog/{}?adjustEstimate=auto",
            self.base_url, issue_key, worklog_id
        );
        self.curl_command("PUT", &url, Some(&payload))
    }

    fn quoted_field(field: &str) -> String {
        if field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            field.to_string()
        } else {
            format!("\"{}\"", field)
        }
    }

    pub fn build_search_jql(
        jira_project: &str,
        sap_field_key: &str,
        sap_number: &str,
        assignee: Option<&str>,
    ) -> String {
        let field = Self::quoted_field(sap_field_key);
        let assignee_clause = if let Some(value) = assignee {
            let escaped = value.replace('"', "\\\"");
            format!("assignee = \"{}\"", escaped)
        } else {
            "assignee = currentUser()".to_string()
        };
        format!(
            "project = {} AND {} ~ \"{}\" AND {} ORDER BY updated DESC",
            jira_project, field, sap_number, assignee_clause
        )
    }
}
