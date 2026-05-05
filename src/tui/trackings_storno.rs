use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::collections::HashSet;

use crate::config::Config;
use crate::db;

enum StornoAction {
    Updated,
    Deleted,
}

pub fn storno_tracking(
    conn: &rusqlite::Connection,
    config: &Config,
    tracking: &db::Tracking,
) -> Result<String> {
    if tracking.jira_synced == 0 {
        return Ok("storno: tracking is already unsynced".to_string());
    }

    let Some((issue_key, worklog_id)) = db::jira_worklog_ref(conn, tracking.id)? else {
        return Ok("storno: missing Jira worklog reference".to_string());
    };

    let jira_url = config
        .jira_url
        .clone()
        .context("jira_url is required for storno")?;
    let jira_token = config
        .jira_token
        .clone()
        .context("jira_token is required for storno")?;

    let (start, seconds) = tracking_timing(tracking)?;
    let subtract_seconds = rounded_worklog_seconds(seconds);
    let tracking_day = start.with_timezone(&Local).date_naive();

    let remove_lines = tracking_description_lines(tracking);
    let keep_lines = other_synced_description_lines(conn, tracking, tracking_day)?;

    let client = crate::jira::JiraClient::new(jira_url, jira_token, config.jira_email.clone());

    crate::jira::set_tracing_enabled(false);
    let jira_result = if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                apply_storno_to_worklog(
                    &client,
                    &issue_key,
                    &worklog_id,
                    &start,
                    subtract_seconds,
                    &remove_lines,
                    &keep_lines,
                    &tracking.project_name,
                )
                .await
            })
        })
    } else {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt.block_on(async {
                apply_storno_to_worklog(
                    &client,
                    &issue_key,
                    &worklog_id,
                    &start,
                    subtract_seconds,
                    &remove_lines,
                    &keep_lines,
                    &tracking.project_name,
                )
                .await
            }),
            Err(err) => Err(err.into()),
        }
    };
    crate::jira::set_tracing_enabled(true);

    let action = match jira_result {
        Ok(action) => action,
        Err(err) if is_worklog_not_found_error(&err) => {
            db::mark_unsynced(conn, tracking.id)?;
            return Ok(format!(
                "storno: jira worklog {} missing on {}, marked unsynced",
                worklog_id, issue_key
            ));
        }
        Err(err) => return Err(err),
    };
    db::mark_unsynced(conn, tracking.id)?;

    let msg = match action {
        StornoAction::Updated => format!(
            "storno: reduced Jira worklog {} on {}",
            worklog_id, issue_key
        ),
        StornoAction::Deleted => format!(
            "storno: removed Jira worklog {} on {} (remaining time 0)",
            worklog_id, issue_key
        ),
    };
    Ok(msg)
}

fn is_worklog_not_found_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("status 404")
        || msg.contains(" not found")
        || msg.contains("nicht gefunden")
        || msg.contains("no longer exists")
}

async fn apply_storno_to_worklog(
    client: &crate::jira::JiraClient,
    issue_key: &str,
    worklog_id: &str,
    tracking_start: &DateTime<Utc>,
    subtract_seconds: i64,
    remove_lines: &[String],
    keep_lines: &HashSet<String>,
    project_name: &str,
) -> Result<StornoAction> {
    let worklogs = client.issue_worklogs(issue_key).await?;
    let Some(existing) = worklogs.into_iter().find(|w| w.id == worklog_id) else {
        bail!("storno failed: jira worklog {} no longer exists", worklog_id);
    };

    let existing_seconds = existing.time_spent_seconds.unwrap_or(0);
    let new_total_seconds = existing_seconds - subtract_seconds;

    if new_total_seconds <= 0 {
        client.delete_worklog(issue_key, worklog_id).await?;
        return Ok(StornoAction::Deleted);
    }

    let started_for_update = existing
        .started
        .parse::<chrono::DateTime<chrono::FixedOffset>>()
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|_| tracking_start.to_rfc3339());

    let comment = reduced_worklog_comment(
        existing.comment.as_ref(),
        remove_lines,
        keep_lines,
        project_name,
    );

    client
        .update_worklog(
            issue_key,
            worklog_id,
            &started_for_update,
            new_total_seconds,
            &comment,
        )
        .await?;
    Ok(StornoAction::Updated)
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
        bail!("tracking has non-positive duration ({}s)", seconds);
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

fn other_synced_description_lines(
    conn: &rusqlite::Connection,
    tracking: &db::Tracking,
    tracking_day: chrono::NaiveDate,
) -> Result<HashSet<String>> {
    let all = db::list_all_trackings(conn)?;
    let mut keep = HashSet::new();

    for row in all {
        if row.id == tracking.id || row.jira_synced == 0 || row.project_name != tracking.project_name {
            continue;
        }
        let Ok(start_dt) = crate::time::parse_ts(&row.start_ts) else {
            continue;
        };
        if start_dt.with_timezone(&Local).date_naive() != tracking_day {
            continue;
        }
        for line in tracking_description_lines(&row) {
            keep.insert(line);
        }
    }

    Ok(keep)
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

fn reduced_worklog_comment(
    existing_comment: Option<&Value>,
    remove_lines: &[String],
    keep_lines: &HashSet<String>,
    project_name: &str,
) -> String {
    let existing_lines = existing_worklog_description_lines(existing_comment);
    let remove_set: HashSet<&str> = remove_lines.iter().map(String::as_str).collect();
    let mut out = Vec::new();

    for line in existing_lines {
        if remove_set.contains(line.as_str()) && !keep_lines.contains(line.as_str()) {
            continue;
        }
        push_unique_line(&mut out, &line);
    }

    if out.is_empty() {
        out.push(format!("LazyTime sync for {}", project_name));
    }
    out.join("\n")
}

fn existing_worklog_description_lines(comment: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(comment) = comment {
        collect_adf_lines(comment, &mut out);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::reduced_worklog_comment;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn storno_keeps_line_when_another_synced_tracking_still_uses_it() {
        let existing = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {"type":"paragraph","content":[{"type":"text","text":"A"}]},
                {"type":"paragraph","content":[{"type":"text","text":"B"}]}
            ]
        });

        let remove = vec!["A".to_string()];
        let mut keep = HashSet::new();
        keep.insert("A".to_string());

        let out = reduced_worklog_comment(Some(&existing), &remove, &keep, "Proj");
        assert_eq!(out, "A\nB");
    }

    #[test]
    fn storno_removes_line_when_unused_by_other_synced_trackings() {
        let existing = json!({
            "type": "doc",
            "version": 1,
            "content": [
                {"type":"paragraph","content":[{"type":"text","text":"A"}]},
                {"type":"paragraph","content":[{"type":"text","text":"B"}]}
            ]
        });

        let remove = vec!["A".to_string()];
        let keep = HashSet::new();

        let out = reduced_worklog_comment(Some(&existing), &remove, &keep, "Proj");
        assert_eq!(out, "B");
    }
}
