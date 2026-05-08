use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Tracking {
    pub id: i64,
    pub project_name: String,
    pub start_ts: String,
    pub end_ts: Option<String>,
    pub created_by: String,
    pub jira_synced: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub sap_number: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectRule {
    pub id: i64,
    pub project_id: i64,
    pub app_id: Option<String>,
    pub name_regex: String,
    pub precedence: i64,
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub day: String,
    pub project_name: String,
    pub seconds: i64,
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create DB parent directory {}", parent.display())
        })?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open sqlite DB {}", path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS projects (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  sap_number TEXT,
  metadata JSON
);

CREATE TABLE IF NOT EXISTS project_rules (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  app_id TEXT,
  -- instance_or_class removed in newer schema
  name_regex TEXT NOT NULL,
  precedence INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS trackings (
  id INTEGER PRIMARY KEY,
  project_name TEXT NOT NULL,
  project_id INTEGER,
  start_ts TEXT NOT NULL,
  end_ts TEXT,
  created_by TEXT NOT NULL,
  window_app_id TEXT,
  window_instance TEXT,
  window_title TEXT,
  workspace TEXT,
  output TEXT,
  jira_synced INTEGER DEFAULT 0,
  jira_issue_key TEXT,
  jira_worklog_id TEXT,
  notes TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_trackings_start_ts ON trackings(start_ts);
CREATE INDEX IF NOT EXISTS idx_trackings_project_name ON trackings(project_name);
CREATE INDEX IF NOT EXISTS idx_trackings_jira_synced ON trackings(jira_synced);

CREATE TABLE IF NOT EXISTS config_store (
  id INTEGER PRIMARY KEY,
  key TEXT UNIQUE,
  value TEXT,
  last_updated TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS schema_migrations (
  id INTEGER PRIMARY KEY,
  version TEXT UNIQUE NOT NULL,
  applied_at TEXT NOT NULL
);
        "#,
    )?;

    let version = "001_initial";
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM schema_migrations WHERE version = ?1",
            params![version],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, crate::time::format_ts(&Utc::now())],
        )?;
    }

    // Migration 002: remove instance_or_class column from project_rules
    let version = "002_remove_instance_or_class";
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM schema_migrations WHERE version = ?1",
            params![version],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        conn.execute_batch(
            r#"
BEGIN;
CREATE TABLE IF NOT EXISTS project_rules_new (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  app_id TEXT,
  name_regex TEXT NOT NULL,
  precedence INTEGER DEFAULT 0
);
INSERT INTO project_rules_new (id, project_id, app_id, name_regex, precedence)
  SELECT id, project_id, app_id, name_regex, precedence FROM project_rules;
DROP TABLE project_rules;
ALTER TABLE project_rules_new RENAME TO project_rules;
COMMIT;
"#,
        )?;

        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, crate::time::format_ts(&Utc::now())],
        )?;
    }

    // Migration 003: enforce single active tracking row (end_ts IS NULL)
    let version = "003_single_active_tracking";
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM schema_migrations WHERE version = ?1",
            params![version],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        conn.execute_batch(
            r#"
BEGIN;
WITH newest AS (
  SELECT id FROM trackings
  WHERE end_ts IS NULL
  ORDER BY start_ts DESC, id DESC
  LIMIT 1
)
UPDATE trackings
SET end_ts = start_ts,
    updated_at = COALESCE(updated_at, start_ts)
WHERE end_ts IS NULL
  AND id <> (SELECT id FROM newest);
CREATE UNIQUE INDEX IF NOT EXISTS idx_trackings_single_active
  ON trackings((1)) WHERE end_ts IS NULL;
COMMIT;
"#,
        )?;

        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, crate::time::format_ts(&Utc::now())],
        )?;
    }

    // Migration 004: add optional project color
    let version = "004_project_color";
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM schema_migrations WHERE version = ?1",
            params![version],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        conn.execute("ALTER TABLE projects ADD COLUMN color TEXT", [])?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, crate::time::format_ts(&Utc::now())],
        )?;
    }

    Ok(())
}

pub fn upsert_config_key(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO config_store (key, value, last_updated) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, last_updated = excluded.last_updated",
        params![key, value, crate::time::format_ts(&Utc::now())],
    )?;
    Ok(())
}

pub fn get_config_key(conn: &Connection, key: &str) -> Result<Option<String>> {
    let value = conn
        .query_row(
            "SELECT value FROM config_store WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(value)
}

pub fn try_acquire_lock(conn: &Connection, key: &str) -> Result<bool> {
    try_acquire_lock_with_value(conn, key, &crate::time::format_ts(&Utc::now()))
}

pub fn try_acquire_lock_with_value(conn: &Connection, key: &str, value: &str) -> Result<bool> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM config_store WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(false);
    }
    let now = crate::time::format_ts(&Utc::now());
    conn.execute(
        "INSERT INTO config_store (key, value, last_updated) VALUES (?1, ?2, ?3)",
        params![key, value, now],
    )?;
    Ok(true)
}

pub fn release_lock(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM config_store WHERE key = ?1", params![key])?;
    Ok(())
}

pub fn release_lock_if_value(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM config_store WHERE key = ?1 AND value = ?2",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_active_tracking(conn: &Connection) -> Result<Option<Tracking>> {
    let tracking = conn
        .query_row(
            "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
             FROM trackings WHERE end_ts IS NULL ORDER BY start_ts DESC LIMIT 1",
            [],
            |r| {
                Ok(Tracking {
                    id: r.get(0)?,
                    project_name: r.get(1)?,
                    start_ts: r.get(2)?,
                    end_ts: r.get(3)?,
                    created_by: r.get(4)?,
                    jira_synced: r.get(5)?,
                    notes: r.get(6).ok(),
                })
            },
        )
        .optional()?;
    Ok(tracking)
}

pub fn start_tracking(
    conn: &mut Connection,
    project_name: &str,
    created_by: &str,
    app_id: Option<&str>,
    instance: Option<&str>,
    title: Option<&str>,
    workspace: Option<&str>,
    output: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
        params![crate::time::format_ts(&now), crate::time::format_ts(&now)],
    )?;
    tx.execute(
        "INSERT INTO trackings (project_name, start_ts, created_by, window_app_id, window_instance, window_title, workspace, output, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            project_name,
            crate::time::format_ts(&now),
            created_by,
            app_id,
            instance,
            title,
            workspace,
            output,
            None::<&str>,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn switch_tracking(
    conn: &mut Connection,
    new_project_name: &str,
    app_id: Option<&str>,
    instance: Option<&str>,
    title: Option<&str>,
    workspace: Option<&str>,
    output: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
        params![crate::time::format_ts(&now), crate::time::format_ts(&now)],
    )?;
    tx.execute(
        "INSERT INTO trackings (project_name, start_ts, created_by, window_app_id, window_instance, window_title, workspace, output, notes)
         VALUES (?1, ?2, 'daemon', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            new_project_name,
            crate::time::format_ts(&now),
            app_id,
            instance,
            title,
            workspace,
            output,
            None::<&str>,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list_today(conn: &Connection) -> Result<Vec<Tracking>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
         FROM trackings
         WHERE date(start_ts) = date('now', 'localtime')
         ORDER BY start_ts ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Tracking {
            id: r.get(0)?,
            project_name: r.get(1)?,
            start_ts: r.get(2)?,
            end_ts: r.get(3)?,
            created_by: r.get(4)?,
            jira_synced: r.get(5)?,
            notes: r.get(6).ok(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn list_all_trackings(conn: &Connection) -> Result<Vec<Tracking>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
         FROM trackings
         ORDER BY start_ts DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Tracking {
            id: r.get(0)?,
            project_name: r.get(1)?,
            start_ts: r.get(2)?,
            end_ts: r.get(3)?,
            created_by: r.get(4)?,
            jira_synced: r.get(5)?,
            notes: r.get(6).ok(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn add_manual_tracking(
    conn: &Connection,
    project_name: &str,
    start_ts: &str,
    end_ts: Option<&str>,
    notes: Option<&str>,
) -> Result<()> {
    if let Some(end) = end_ts
        && let Some(day) = day_key_from_ts(start_ts)
        && has_overlap_for_day(conn, &day, None, start_ts, end)?
    {
        return Err(anyhow!("tracking overlaps existing entry"));
    }
    if end_ts.is_none() {
        let now = crate::time::format_ts(&Utc::now());
        conn.execute(
            "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
            params![now, now],
        )?;
    }
    conn.execute(
        "INSERT INTO trackings (project_name, start_ts, end_ts, created_by, notes)
         VALUES (?1, ?2, ?3, 'tui', ?4)",
        params![project_name, start_ts, end_ts, notes],
    )?;
    Ok(())
}

pub fn update_tracking_times(
    conn: &Connection,
    tracking_id: i64,
    project_name: &str,
    start_ts: &str,
    end_ts: Option<&str>,
    notes: Option<&str>,
) -> Result<()> {
    if let Some(end) = end_ts
        && let Some(day) = day_key_from_ts(start_ts)
        && has_overlap_for_day(conn, &day, Some(tracking_id), start_ts, end)?
    {
        return Err(anyhow!("tracking overlaps existing entry"));
    }
    if end_ts.is_none() {
        let now = crate::time::format_ts(&Utc::now());
        conn.execute(
            "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL AND id <> ?3",
            params![now, now, tracking_id],
        )?;
    }
    conn.execute(
        "UPDATE trackings
         SET project_name = ?1, start_ts = ?2, end_ts = ?3, notes = ?4, updated_at = ?5
         WHERE id = ?6",
        params![
            project_name,
            start_ts,
            end_ts,
            notes,
            crate::time::format_ts(&Utc::now()),
            tracking_id
        ],
    )?;
    Ok(())
}

pub fn set_tracking_synced(conn: &Connection, tracking_id: i64, val: i64) -> Result<()> {
    conn.execute(
        "UPDATE trackings SET jira_synced = ?1, updated_at = ?2 WHERE id = ?3",
        params![val, crate::time::format_ts(&Utc::now()), tracking_id],
    )?;
    Ok(())
}

pub fn delete_tracking(conn: &Connection, tracking_id: i64) -> Result<()> {
    conn.execute("DELETE FROM trackings WHERE id = ?1", params![tracking_id])?;
    Ok(())
}

pub fn bump_rule_precedence(conn: &Connection, rule_id: i64, delta: i64) -> Result<()> {
    conn.execute(
        "UPDATE project_rules
         SET precedence = precedence + ?1
         WHERE id = ?2",
        params![delta, rule_id],
    )?;
    Ok(())
}

pub fn list_unsynced_finished(conn: &Connection) -> Result<Vec<Tracking>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
         FROM trackings
         WHERE jira_synced = 0 AND end_ts IS NOT NULL
         ORDER BY start_ts ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Tracking {
            id: r.get(0)?,
            project_name: r.get(1)?,
            start_ts: r.get(2)?,
            end_ts: r.get(3)?,
            created_by: r.get(4)?,
            jira_synced: r.get(5)?,
            notes: r.get(6).ok(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn mark_synced(
    conn: &Connection,
    tracking_id: i64,
    jira_issue_key: &str,
    jira_worklog_id: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE trackings SET jira_synced = 1, jira_issue_key = ?1, jira_worklog_id = ?2, updated_at = ?3 WHERE id = ?4",
        params![jira_issue_key, jira_worklog_id, crate::time::format_ts(&Utc::now()), tracking_id],
    )?;
    Ok(())
}

pub fn jira_worklog_ref(conn: &Connection, tracking_id: i64) -> Result<Option<(String, String)>> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT jira_issue_key, jira_worklog_id FROM trackings WHERE id = ?1 AND jira_synced != 0",
            params![tracking_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(issue, worklog)| match (issue, worklog) {
        (Some(i), Some(w)) if !i.trim().is_empty() && !w.trim().is_empty() => Some((i, w)),
        _ => None,
    }))
}

pub fn mark_unsynced(conn: &Connection, tracking_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE trackings SET jira_synced = 0, jira_issue_key = NULL, jira_worklog_id = NULL, updated_at = ?1 WHERE id = ?2",
        params![crate::time::format_ts(&Utc::now()), tracking_id],
    )?;
    Ok(())
}

pub fn projects(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt =
        conn.prepare("SELECT id, name, sap_number, color FROM projects ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(Project {
            id: r.get(0)?,
            name: r.get(1)?,
            sap_number: r.get(2)?,
            color: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn project_rules(conn: &Connection) -> Result<Vec<ProjectRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, app_id, name_regex, precedence
         FROM project_rules
         ORDER BY precedence ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ProjectRule {
            id: r.get(0)?,
            project_id: r.get(1)?,
            app_id: r.get(2)?,
            name_regex: r.get(3)?,
            precedence: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn project_sap_number(conn: &Connection, project_name: &str) -> Result<Option<String>> {
    let sap = conn
        .query_row(
            "SELECT sap_number FROM projects WHERE name = ?1",
            params![project_name],
            |r| r.get(0),
        )
        .optional()?;
    Ok(sap)
}

pub fn ensure_project(conn: &Connection, name: &str, sap_number: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO projects (name, sap_number) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET sap_number = excluded.sap_number",
        params![name, sap_number],
    )?;
    Ok(())
}

pub fn replace_rules(
    conn: &mut Connection,
    project_name: &str,
    sap_number: Option<&str>,
    rules: &[(&str, Option<&str>, &str, i64)],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO projects (name, sap_number) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET sap_number = excluded.sap_number",
        params![project_name, sap_number],
    )?;
    let project_id: i64 = tx.query_row(
        "SELECT id FROM projects WHERE name = ?1",
        params![project_name],
        |r| r.get(0),
    )?;
    tx.execute(
        "DELETE FROM project_rules WHERE project_id = ?1",
        params![project_id],
    )?;
    for (app_id, _instance_or_class, regex, precedence) in rules {
        tx.execute(
            "INSERT INTO project_rules (project_id, app_id, name_regex, precedence)
             VALUES (?1, ?2, ?3, ?4)",
            params![project_id, app_id, regex, precedence],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn report_range(conn: &Connection, start: &str, end: &str) -> Result<Vec<ReportRow>> {
    let mut stmt = conn.prepare(
        "SELECT date(start_ts) AS day,
                project_name,
                CAST(SUM(strftime('%s', COALESCE(end_ts, CURRENT_TIMESTAMP)) - strftime('%s', start_ts)) AS INTEGER) AS seconds
         FROM trackings
         WHERE date(start_ts) >= date(?1) AND date(start_ts) <= date(?2)
         GROUP BY day, project_name
         ORDER BY day ASC, project_name ASC",
    )?;
    let rows = stmt.query_map(params![start, end], |r| {
        Ok(ReportRow {
            day: r.get(0)?,
            project_name: r.get(1)?,
            seconds: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn add_project(
    conn: &Connection,
    name: &str,
    sap_number: Option<&str>,
    color: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO projects (name, sap_number, color) VALUES (?1, ?2, ?3)",
        params![name, sap_number, color],
    )?;
    Ok(())
}

pub fn update_project(
    conn: &Connection,
    project_id: i64,
    name: &str,
    sap_number: Option<&str>,
    color: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE projects SET name = ?1, sap_number = ?2, color = ?3 WHERE id = ?4",
        params![name, sap_number, color, project_id],
    )?;
    Ok(())
}

pub fn project_color_by_name(conn: &Connection, project_name: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT color FROM projects WHERE name = ?1",
        params![project_name],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn list_trackings_for_date(conn: &Connection, date: &str) -> Result<Vec<Tracking>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
         FROM trackings
         WHERE date(start_ts) = date(?1)
         ORDER BY start_ts ASC",
    )?;
    let rows = stmt.query_map(params![date], |r| {
        Ok(Tracking {
            id: r.get(0)?,
            project_name: r.get(1)?,
            start_ts: r.get(2)?,
            end_ts: r.get(3)?,
            created_by: r.get(4)?,
            jira_synced: r.get(5)?,
            notes: r.get(6).ok(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn list_trackings_for_range(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<Tracking>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_name, start_ts, end_ts, created_by, jira_synced, notes
         FROM trackings
         WHERE date(start_ts) >= date(?1) AND date(start_ts) <= date(?2)
         ORDER BY start_ts ASC",
    )?;
    let rows = stmt.query_map(params![start_date, end_date], |r| {
        Ok(Tracking {
            id: r.get(0)?,
            project_name: r.get(1)?,
            start_ts: r.get(2)?,
            end_ts: r.get(3)?,
            created_by: r.get(4)?,
            jira_synced: r.get(5)?,
            notes: r.get(6).ok(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn has_overlap_for_day(
    conn: &Connection,
    day: &str,
    exclude_tracking_id: Option<i64>,
    candidate_start_ts: &str,
    candidate_end_ts: &str,
) -> Result<bool> {
    let overlap: Option<i64> = conn
        .query_row(
            "SELECT id FROM trackings
             WHERE date(start_ts) = date(?1)
               AND (?2 IS NULL OR id <> ?2)
               AND ?3 < COALESCE(end_ts, start_ts)
               AND start_ts < ?4
             LIMIT 1",
            params![
                day,
                exclude_tracking_id,
                candidate_start_ts,
                candidate_end_ts
            ],
            |r| r.get(0),
        )
        .optional()?;
    Ok(overlap.is_some())
}

fn day_key_from_ts(ts: &str) -> Option<String> {
    crate::time::parse_ts(ts).ok().map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
}

pub fn delete_project(conn: &Connection, project_id: i64) -> Result<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
    Ok(())
}

pub fn add_rule(
    conn: &Connection,
    project_id: i64,
    app_id: Option<&str>,
    _instance_or_class: Option<&str>,
    name_regex: &str,
    precedence: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO project_rules (project_id, app_id, name_regex, precedence)
         VALUES (?1, ?2, ?3, ?4)",
        params![project_id, app_id, name_regex, precedence],
    )?;
    Ok(())
}

pub fn update_rule(
    conn: &Connection,
    rule_id: i64,
    app_id: Option<&str>,
    _instance_or_class: Option<&str>,
    name_regex: &str,
    precedence: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE project_rules
         SET app_id = ?1, name_regex = ?2, precedence = ?3
         WHERE id = ?4",
        params![app_id, name_regex, precedence, rule_id],
    )?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, rule_id: i64) -> Result<()> {
    conn.execute("DELETE FROM project_rules WHERE id = ?1", params![rule_id])?;
    Ok(())
}

pub fn rules_for_project(conn: &Connection, project_id: i64) -> Result<Vec<ProjectRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, app_id, name_regex, precedence
         FROM project_rules WHERE project_id = ?1 ORDER BY precedence ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok(ProjectRule {
            id: r.get(0)?,
            project_id: r.get(1)?,
            app_id: r.get(2)?,
            name_regex: r.get(3)?,
            precedence: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
