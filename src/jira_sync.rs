use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::mpsc::Sender;

use crate::{config::Config, db, jira};

#[derive(Debug, Clone)]
pub enum JiraSyncEvent {
    Log(String),
    Progress { processed: usize, total: usize },
    Finished { success: bool, message: String },
}

const JIRA_SYNC_LOCK_KEY: &str = "jira_sync_lock";

fn emit_event(sender: Option<&Sender<JiraSyncEvent>>, event: JiraSyncEvent) {
    if let Some(tx) = sender {
        let _ = tx.send(event);
    }
}

fn emit_log(sender: Option<&Sender<JiraSyncEvent>>, line: String) {
    if sender.is_some() {
        // When running under the TUI, send the line to the UI and avoid
        // printing via tracing/println to keep terminal layout intact.
        emit_event(sender, JiraSyncEvent::Log(line));
    } else {
        tracing::info!("{}", line);
        println!("{}", line);
    }
}

fn emit_debug(sender: Option<&Sender<JiraSyncEvent>>, line: String) {
    if sender.is_some() {
        // Send debug to UI only
        emit_event(sender, JiraSyncEvent::Log(line));
    } else {
        tracing::debug!("{}", line);
    }
}

fn emit_error(sender: Option<&Sender<JiraSyncEvent>>, line: String) {
    if sender.is_some() {
        emit_event(sender, JiraSyncEvent::Log(line));
    } else {
        tracing::error!("{}", line);
        println!("{}", line);
    }
}

fn sync_disabled_message() -> String {
    "jira sync already running; skipping new trigger".to_string()
}

fn tracking_timing(tracking: &db::Tracking) -> Result<(DateTime<Utc>, i64)> {
    let end_ts = tracking
        .end_ts
        .as_ref()
        .context("tracking missing end_ts")?;
    let start = crate::time::parse_ts(&tracking.start_ts).context("invalid tracking start_ts")?;
    let end = crate::time::parse_ts(end_ts).context("invalid tracking end_ts")?;
    let seconds = end.signed_duration_since(start).num_seconds();
    if seconds <= 0 {
        anyhow::bail!("tracking has non-positive duration ({}s)", seconds);
    }
    Ok((start, seconds))
}

fn rounded_worklog_seconds(seconds: i64) -> i64 {
    if seconds <= 0 {
        60
    } else {
        ((seconds + 59) / 60) * 60
    }
}

fn is_lazytime_comment(comment: Option<&serde_json::Value>) -> bool {
    if let Some(c) = comment {
        if let Ok(raw) = serde_json::to_string(c) {
            return raw.contains("LazyTime sync");
        }
    }
    false
}

fn worklog_local_day(started: &str) -> Option<chrono::NaiveDate> {
    let dt: chrono::DateTime<chrono::FixedOffset> = started.parse().ok()?;
    Some(dt.with_timezone(&Local).date_naive())
}

fn push_unique_line(lines: &mut Vec<String>, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if !lines.iter().any(|existing| existing == trimmed) {
        lines.push(trimmed.to_string());
    }
}

fn collect_adf_lines(node: &Value, lines: &mut Vec<String>) {
    if let Some(obj) = node.as_object() {
        let node_type = obj.get("type").and_then(|v| v.as_str());
        if node_type == Some("paragraph") {
            let mut paragraph_text = String::new();
            if let Some(children) = obj.get("content").and_then(|v| v.as_array()) {
                for child in children {
                    if let Some(child_obj) = child.as_object() {
                        match child_obj.get("type").and_then(|v| v.as_str()) {
                            Some("text") => {
                                if let Some(text) = child_obj.get("text").and_then(|v| v.as_str()) {
                                    paragraph_text.push_str(text);
                                }
                            }
                            Some("hardBreak") => {
                                paragraph_text.push('\n');
                            }
                            _ => {
                                collect_adf_lines(child, lines);
                            }
                        }
                    }
                }
            }
            for line in paragraph_text.lines() {
                push_unique_line(lines, line);
            }
            return;
        }
        if let Some(children) = obj.get("content").and_then(|v| v.as_array()) {
            for child in children {
                collect_adf_lines(child, lines);
            }
        }
    }
}

fn existing_worklog_description_lines(comment: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(comment) = comment {
        collect_adf_lines(comment, &mut out);
    }
    out
}

fn tracking_description_lines(tracking: &db::Tracking) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(notes) = tracking.notes.as_deref() {
        for line in notes.lines() {
            push_unique_line(&mut out, line);
        }
    }
    if out.is_empty() {
        out.push(format!("LazyTime sync for {}", tracking.project_name));
    }
    out
}

fn merged_worklog_comment(existing_comment: Option<&Value>, tracking: &db::Tracking) -> String {
    let mut lines = existing_worklog_description_lines(existing_comment);
    for line in tracking_description_lines(tracking) {
        push_unique_line(&mut lines, &line);
    }
    if lines.is_empty() {
        lines.push(format!("LazyTime sync for {}", tracking.project_name));
    }
    lines.join("\n")
}

async fn ensure_issue_for_sap(
    client: &jira::JiraClient,
    sender: Option<&Sender<JiraSyncEvent>>,
    cache: &mut HashMap<String, String>,
    config: &Config,
    tracking: &db::Tracking,
    jira_project: &str,
    assignee: Option<&str>,
    sap_number: &str,
    dry_run: bool,
) -> Result<String> {
    if let Some(issue) = cache.get(sap_number) {
        emit_debug(
            sender,
            format!(
                "reusing cached issue {} for tracking {} project={} sap={}",
                issue, tracking.id, tracking.project_name, sap_number
            ),
        );
        return Ok(issue.clone());
    }

    // If a jira_assignee is configured, try to resolve it to an accountId and use that for search
    let resolved_assignee_account_id: Option<String> = if let Some(a) = assignee {
        match client.resolve_account_id(a).await {
            Ok(Some(id)) => {
                emit_debug(sender, format!("resolved configured jira_assignee '{}' -> accountId {}", a, id));
                Some(id)
            }
            Ok(None) => {
                // couldn't resolve; fall back to no assignee filter for search
                emit_debug(sender, format!("could not resolve configured jira_assignee '{}'; searching without assignee filter", a));
                None
            }
            Err(err) => {
                emit_debug(sender, format!("warning: failed to resolve jira assignee '{}' -> {} ; searching without assignee filter", a, err));
                None
            }
        }
    } else {
        None
    };

    // Try searching for an existing issue using the resolved assignee (accountId) when available.
    let mut found_issue: Option<String> = None;
    let search_assignee_for_curl = resolved_assignee_account_id.as_deref();
    match client
        .find_issue(jira_project, sap_number, &config.jira_sap_field, search_assignee_for_curl)
        .await
    {
            Ok(Some(issue)) => {
                let curl_search = client.curl_for_search(
                    jira_project,
                    &config.jira_sap_field,
                    sap_number,
                    search_assignee_for_curl,
                );
                if sender.is_none() {
                    tracing::debug!(curl = %curl_search, "jira request curl");
                }
            emit_debug(
                sender,
                format!(
                    "reusing existing issue {} for tracking {} project={} sap={}",
                    issue, tracking.id, tracking.project_name, sap_number
                ),
            );
            found_issue = Some(issue);
        }
        Ok(None) => {
            // If we searched with an assignee filter and found nothing, try again without the assignee filter.
                if search_assignee_for_curl.is_some() {
                emit_debug(sender, format!("no issue found using assignee filter; trying search without assignee"));
                let curl_search =
                    client.curl_for_search(jira_project, &config.jira_sap_field, sap_number, None);
                if sender.is_none() {
                    tracing::debug!(curl = %curl_search, "jira request curl");
                }
                if let Some(issue) = client
                    .find_issue(jira_project, sap_number, &config.jira_sap_field, None)
                    .await?
                {
                    emit_debug(
                        sender,
                        format!(
                            "reusing existing issue {} for tracking {} project={} sap={}",
                            issue, tracking.id, tracking.project_name, sap_number
                        ),
                    );
                    found_issue = Some(issue);
                }
            }
        }
        Err(err) => return Err(err),
    }

    if let Some(issue) = found_issue {
        cache.insert(sap_number.to_string(), issue.clone());
        return Ok(issue);
    }

    if dry_run {
        emit_debug(
            sender,
            format!(
                "dry-run: would create issue in {} type={} {}={}",
                jira_project, config.jira_issue_type, config.jira_sap_field, sap_number
            ),
        );
        let issue = format!("DRYRUN-{}", tracking.id);
        cache.insert(sap_number.to_string(), issue.clone());
        return Ok(issue);
    }

    let summary = format!("{} tracking", tracking.project_name);
    let description = format!(
        "Auto-created by LazyTime for project {} (sap_number={})",
        tracking.project_name, sap_number
    );
    emit_debug(
        sender,
        format!(
            "Creating Jira issue for tracking {} (project={}, sap={})",
            tracking.id, jira_project, sap_number
        ),
    );
    // Attempt to resolve configured assignee to an accountId; fall back to name if unresolved.
    let mut assignee_account_id: Option<String> = None;
    let mut assignee_name_for_create: Option<&str> = None;
    if let Some(a) = assignee {
        match client.resolve_account_id(a).await {
            Ok(Some(id)) => {
                assignee_account_id = Some(id);
                emit_debug(sender, format!("assigning created issue to accountId {}", assignee_account_id.as_ref().unwrap()));
            }
            Ok(None) => {
                assignee_name_for_create = Some(a);
                emit_debug(sender, format!("assigning created issue to user name '{}' (accountId not resolved)", a));
            }
            Err(err) => {
                assignee_name_for_create = Some(a);
                emit_debug(sender, format!("warning: failed to resolve jira assignee '{}' -> {} ; will use name fallback", a, err));
            }
        }
    }

    // Emit curl for create so user can replicate the exact create payload
    let curl_create = client.curl_for_create_issue(
        jira_project,
        assignee_account_id.as_deref(),
        assignee_name_for_create,
        &summary,
        &description,
        &config.jira_issue_type,
        &config.jira_sap_field,
        sap_number,
    );
    if sender.is_none() {
        tracing::debug!(curl = %curl_create, "jira request curl");
    }
    let issue = client
        .create_issue(
            jira_project,
            assignee_account_id.as_deref(),
            assignee_name_for_create,
            &summary,
            &description,
            &config.jira_issue_type,
            &config.jira_sap_field,
            sap_number,
        )
        .await?;
    emit_log(sender, format!("Created issue {} for tracking {}", issue, tracking.id));
    cache.insert(sap_number.to_string(), issue.clone());
    Ok(issue)
}

pub async fn run_jira_sync(
    config: &Config,
    dry_run: bool,
    sender: Option<Sender<JiraSyncEvent>>,
) -> Result<()> {
    let sender_ref = sender.as_ref();
    let jira_url = config
        .jira_url
        .clone()
        .context("jira_url is required for --jira-sync")?;
    let jira_token = config
        .jira_token
        .clone()
        .context("jira_token is required for --jira-sync")?;
    let jira_project = config
        .jira_project
        .clone()
        .context("jira_project is required for --jira-sync")?;

    let client = jira::JiraClient::new(jira_url, jira_token, config.jira_email.clone());
    let assignee = config.jira_assignee.as_deref();
    let conn = db::open(config.db_path())?;
    let my_account_id = match client.authenticated_account_id().await {
        Ok(id) => {
            emit_debug(sender_ref, format!("resolved jira authenticated accountId={}", id));
            Some(id)
        }
        Err(err) => {
            emit_debug(
                sender_ref,
                format!(
                    "warning: failed to resolve authenticated jira accountId: {}",
                    err
                ),
            );
            None
        }
    };

    let lock_held = if dry_run {
        false
    } else {
        let acquired = db::try_acquire_lock(&conn, JIRA_SYNC_LOCK_KEY)?;
        if !acquired {
            let msg = sync_disabled_message();
            emit_log(sender_ref, msg.clone());
            emit_event(
                sender_ref,
                JiraSyncEvent::Finished {
                    success: false,
                    message: msg,
                },
            );
            return Ok(());
        }
        true
    };

    let trackings = db::list_unsynced_finished(&conn)?;
    let total = trackings.len();
    emit_event(
        sender_ref,
        JiraSyncEvent::Progress {
            processed: 0,
            total,
        },
    );
    emit_log(sender_ref, format!("Jira sync started: {} trackings", total));

    let mut processed = 0usize;
    let mut issue_cache: HashMap<String, String> = HashMap::new();

    let result = async {
        for tracking in trackings {
            emit_debug(
                sender_ref,
                format!(
                    "processing tracking {} project={} start={}",
                    tracking.id, tracking.project_name, tracking.start_ts
                ),
            );

            let sap_number = match db::project_sap_number(&conn, &tracking.project_name)? {
                Some(sap) => sap,
                None => {
                    emit_debug(
                        sender_ref,
                        format!(
                            "warning: skipping tracking {} project={} due to missing sap_number",
                            tracking.id, tracking.project_name
                        ),
                    );
                    processed += 1;
                    emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                    continue;
                }
            };

            let issue_key = match ensure_issue_for_sap(
                &client,
                sender_ref,
                &mut issue_cache,
                config,
                &tracking,
                &jira_project,
                assignee,
                &sap_number,
                dry_run,
            )
            .await
            {
                Ok(key) => key,
                Err(err) => {
                    emit_debug(
                        sender_ref,
                        format!("error: tracking {} search/create failed: {}", tracking.id, err),
                    );
                    processed += 1;
                    emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                    continue;
                }
            };

            let (start, seconds) = match tracking_timing(&tracking) {
                Ok(v) => v,
                Err(err) => {
                    emit_debug(
                        sender_ref,
                        format!("error: tracking {} timing invalid: {}", tracking.id, err),
                    );
                    processed += 1;
                    emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                    continue;
                }
            };

            let start_local_day = start.with_timezone(&Local).date_naive();
            let existing_worklogs = match client.issue_worklogs(&issue_key).await {
                Ok(v) => v,
                Err(err) => {
                    emit_debug(
                        sender_ref,
                        format!(
                            "warning: failed to list worklogs for issue {}: {}; will create new worklog",
                            issue_key, err
                        ),
                    );
                    Vec::new()
                }
            };

            let mut matching_worklog: Option<jira::WorklogItem> = None;
            if let Some(my_id) = my_account_id.as_deref() {
                matching_worklog = existing_worklogs
                    .iter()
                    .find(|w| {
                        let same_author = w
                            .author
                            .as_ref()
                            .and_then(|a| a.account_id.as_deref())
                            .map(|id| id == my_id)
                            .unwrap_or(false);
                        let same_day = worklog_local_day(&w.started)
                            .map(|d| d == start_local_day)
                            .unwrap_or(false);
                        same_author && same_day
                    })
                    .cloned();
            }

            if matching_worklog.is_none() {
                matching_worklog = existing_worklogs
                    .iter()
                    .find(|w| {
                        let same_day = worklog_local_day(&w.started)
                            .map(|d| d == start_local_day)
                            .unwrap_or(false);
                        same_day && is_lazytime_comment(w.comment.as_ref())
                    })
                    .cloned();
            }

            if let Some(existing) = matching_worklog {
                let existing_seconds = existing.time_spent_seconds.unwrap_or(0);
                let add_seconds = rounded_worklog_seconds(seconds);
                let new_total_seconds = existing_seconds + add_seconds;

                let started_for_update = existing
                    .started
                    .parse::<chrono::DateTime<chrono::FixedOffset>>()
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|_| start.to_rfc3339());

                let comment = merged_worklog_comment(existing.comment.as_ref(), &tracking);
                let curl_update = client.curl_for_update_worklog(
                    &issue_key,
                    &existing.id,
                    &started_for_update,
                    new_total_seconds,
                    &comment,
                );
                if sender_ref.is_none() {
                    tracing::debug!(curl = %curl_update, "jira request curl");
                }

                if dry_run {
                    emit_debug(
                        sender_ref,
                        format!(
                            "dry-run: would update worklog issue={} worklog={} day={} existing_seconds={} add_seconds={} new_total_seconds={}",
                            issue_key,
                            existing.id,
                            start_local_day,
                            existing_seconds,
                            add_seconds,
                            new_total_seconds
                        ),
                    );
                    processed += 1;
                    emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                    continue;
                }

                emit_debug(
                    sender_ref,
                    format!(
                        "updating existing worklog issue={} worklog={} tracking={} day={} existing_seconds={} add_seconds={} new_total_seconds={}",
                        issue_key,
                        existing.id,
                        tracking.id,
                        start_local_day,
                        existing_seconds,
                        add_seconds,
                        new_total_seconds
                    ),
                );

                let worklog_id = match client
                    .update_worklog(
                        &issue_key,
                        &existing.id,
                        &started_for_update,
                        new_total_seconds,
                        &comment,
                    )
                    .await
                {
                    Ok(id) => id,
                    Err(err) => {
                        emit_debug(
                            sender_ref,
                            format!(
                                "warning: tracking {} update worklog failed: {}; falling back to create",
                                tracking.id, err
                            ),
                        );
                        let fallback_id = match client
                            .add_worklog(
                                &issue_key,
                                &start.to_rfc3339(),
                                seconds,
                                &comment,
                            )
                            .await
                        {
                            Ok(id) => id,
                            Err(add_err) => {
                                emit_debug(
                                    sender_ref,
                                    format!(
                                        "error: tracking {} add worklog failed after update fallback: {}",
                                        tracking.id, add_err
                                    ),
                                );
                                processed += 1;
                                emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                                continue;
                            }
                        };
                        fallback_id
                    }
                };

                db::mark_synced(&conn, tracking.id, &issue_key, &worklog_id)?;
                emit_log(
                    sender_ref,
                    format!(
                        "Worklog updated for tracking {}: issue={} worklog={}",
                        tracking.id, issue_key, worklog_id
                    ),
                );
                processed += 1;
                emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                continue;
            }

            // Emit a ready-to-run curl command (contains placeholder for $JIRA_TOKEN) so user can replicate
            let curl = client.curl_for_add_worklog(
                &issue_key,
                &start.to_rfc3339(),
                seconds,
                    &merged_worklog_comment(None, &tracking),
                );
            if sender_ref.is_none() {
                tracing::debug!(curl = %curl, "jira request curl");
            }

            if dry_run {
                emit_debug(
                    sender_ref,
                    format!(
                        "dry-run: would add worklog issue={} start={} seconds={}",
                        issue_key,
                        start.to_rfc3339(),
                        seconds
                    ),
                );
                processed += 1;
                emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                continue;
            }

            emit_debug(
                sender_ref,
                format!(
                    "adding worklog to issue={} for tracking {} start={} seconds={}",
                    issue_key,
                    tracking.id,
                    start.to_rfc3339(),
                    seconds
                ),
            );
            let worklog_id = match client
                .add_worklog(
                    &issue_key,
                    &start.to_rfc3339(),
                    seconds,
                    &format!("LazyTime sync for {}", tracking.project_name),
                )
                .await
            {
                Ok(id) => id,
                Err(err) => {
                    emit_debug(
                        sender_ref,
                        format!("error: tracking {} add worklog failed: {}", tracking.id, err),
                    );
                    processed += 1;
                    emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
                    continue;
                }
            };

            db::mark_synced(&conn, tracking.id, &issue_key, &worklog_id)?;
            emit_log(
                sender_ref,
                format!(
                    "Worklog added for tracking {}: issue={} worklog={}",
                    tracking.id, issue_key, worklog_id
                ),
            );
            processed += 1;
            emit_event(sender_ref, JiraSyncEvent::Progress { processed, total });
        }
        Ok(())
    }
    .await;

    if lock_held {
        let _ = db::release_lock(&conn, JIRA_SYNC_LOCK_KEY);
    }

    match result {
        Ok(()) => {
            emit_log(sender_ref, "Jira sync finished".to_string());
            emit_event(
                sender_ref,
                JiraSyncEvent::Finished {
                    success: true,
                    message: "jira sync finished".to_string(),
                },
            );
            Ok(())
        }
        Err(err) => {
            let message = format!("jira sync failed: {}", err);
            emit_error(sender_ref, message.clone());
            emit_event(
                sender_ref,
                JiraSyncEvent::Finished {
                    success: false,
                    message,
                },
            );
            Err(err)
        }
    }
}
