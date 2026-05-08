use anyhow::Result;
use chrono::{Local, NaiveDate, Timelike};

use crate::db;

#[derive(Debug, Clone, Copy, Default)]
pub struct CleanupStats {
    pub merged_groups: usize,
    pub removed_rows: usize,
}

pub fn cleanup_unsynced_trackings_in_range(
    conn: &rusqlite::Connection,
    filter_start: &str,
    filter_end: &str,
) -> Result<CleanupStats> {
    let today = Local::now().date_naive();
    let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let start_day = parse_day(filter_start).unwrap_or(today);
    let end_day = parse_day(filter_end).unwrap_or(start_day);
    let (from_day, to_day) = if start_day <= end_day {
        (start_day, end_day)
    } else {
        (end_day, start_day)
    };
    let trackings = db::list_trackings_for_range(
        conn,
        &from_day.format("%Y-%m-%d").to_string(),
        &to_day.format("%Y-%m-%d").to_string(),
    )?;
    let mut stats = CleanupStats::default();
    let mut group: Vec<db::Tracking> = Vec::new();

    let flush = |group: &mut Vec<db::Tracking>, stats: &mut CleanupStats| -> Result<()> {
        if group.len() <= 1 {
            group.clear();
            return Ok(());
        }
        let first = group[0].clone();
        let last_end = group.last().and_then(|t| t.end_ts.as_deref());
        for t in group.iter().skip(1) {
            db::delete_tracking(conn, t.id)?;
            stats.removed_rows += 1;
        }
        db::update_tracking_times(
            conn,
            first.id,
            &first.project_name,
            &first.start_ts,
            last_end,
            first.notes.as_deref(),
        )?;
        stats.merged_groups += 1;
        group.clear();
        Ok(())
    };

    for t in trackings {
        if t.jira_synced != 0 {
            flush(&mut group, &mut stats)?;
            continue;
        }
        if t.end_ts.is_none() {
            flush(&mut group, &mut stats)?;
            continue;
        }

        if let Some(prev) = group.last() {
            if !can_merge(prev, &t) {
                flush(&mut group, &mut stats)?;
            }
        }
        group.push(t);
    }
    flush(&mut group, &mut stats)?;

    Ok(stats)
}

fn can_merge(prev: &db::Tracking, next: &db::Tracking) -> bool {
    if prev.project_name != next.project_name {
        return false;
    }
    let Some(prev_end) = prev.end_ts.as_deref() else {
        return false;
    };
    let Ok(prev_end_dt) = crate::time::parse_ts(prev_end) else {
        return false;
    };
    let Ok(next_start_dt) = crate::time::parse_ts(&next.start_ts) else {
        return false;
    };

    let prev_local = prev_end_dt.with_timezone(&Local);
    let next_local = next_start_dt.with_timezone(&Local);
    let prev_day = prev_local.date_naive();
    let next_day = next_local.date_naive();
    if prev_day != next_day {
        return false;
    }

    let prev_min = prev_local
        .with_second(0)
        .and_then(|dt| dt.with_nanosecond(0));
    let next_min = next_local
        .with_second(0)
        .and_then(|dt| dt.with_nanosecond(0));

    prev_min.is_some() && prev_min == next_min
}
