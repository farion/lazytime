use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate, Timelike};

use crate::config::{Config, parse_hhmm};
use crate::db;

#[derive(Debug, Clone)]
pub enum DisplayRow {
    Tracking(db::Tracking),
    Gap(GapRow),
    Separator,
}

#[derive(Debug, Clone)]
pub struct GapRow {
    pub start_ts: String,
    pub end_ts: String,
    pub previous_project: String,
}

pub fn display_rows(
    conn: &rusqlite::Connection,
    config: &Config,
    filter_start: &str,
    filter_end: &str,
    show_gaps: bool,
) -> Result<Vec<DisplayRow>> {
    let trackings = db::list_all_trackings(conn)?;

    let start_day = parse_day(filter_start).unwrap_or_else(|| Local::now().date_naive());
    let end_day = parse_day(filter_end).unwrap_or(start_day);
    let (from_day, to_day) = if start_day <= end_day {
        (start_day, end_day)
    } else {
        (end_day, start_day)
    };

    let filtered: Vec<db::Tracking> = trackings
        .into_iter()
        .filter(|t| {
            let Ok(dt) = crate::time::parse_ts(&t.start_ts) else {
                return false;
            };
            let day = dt.with_timezone(&Local).date_naive();
            day >= from_day && day <= to_day
        })
        .collect();

    let mut out = Vec::new();
    let mut prev_day: Option<NaiveDate> = None;
    for (idx, t) in filtered.iter().enumerate() {
        let day = crate::time::parse_ts(&t.start_ts)
            .map(|dt| dt.with_timezone(&Local).date_naive())
            .ok();
        if let (Some(prev), Some(cur)) = (prev_day, day)
            && prev != cur
        {
            out.push(DisplayRow::Separator);
        }
        out.push(DisplayRow::Tracking(t.clone()));

        if show_gaps
            && idx + 1 < filtered.len()
            && let Some(gap) = build_gap_row(&filtered[idx + 1], t, config)
        {
            out.push(DisplayRow::Gap(gap));
        }
        prev_day = day;
    }

    Ok(out)
}

pub fn extract_date(raw: &str) -> Option<String> {
    if let Ok(dt) = crate::time::parse_local_ts(raw) {
        return Some(
            dt.with_timezone(&Local)
                .date_naive()
                .format("%Y-%m-%d")
                .to_string(),
        );
    }
    if raw.len() >= 10 {
        let head = &raw[0..10];
        if NaiveDate::parse_from_str(head, "%Y-%m-%d").is_ok() {
            return Some(head.to_string());
        }
    }
    None
}

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

fn build_gap_row(prev: &db::Tracking, next: &db::Tracking, config: &Config) -> Option<GapRow> {
    let prev_end = prev.end_ts.as_ref()?;
    let prev_end_dt = crate::time::parse_ts(prev_end).ok()?;
    let next_start_dt = crate::time::parse_ts(&next.start_ts).ok()?;
    if next_start_dt <= prev_end_dt {
        return None;
    }

    let gap_secs = next_start_dt
        .signed_duration_since(prev_end_dt)
        .num_seconds();
    if gap_secs <= 60 {
        return None;
    }

    let prev_local = prev_end_dt.with_timezone(&Local);
    let next_local = next_start_dt.with_timezone(&Local);
    if prev_local.date_naive() != next_local.date_naive() {
        return None;
    }
    if !within_same_working_slice(prev_local, next_local, config) {
        return None;
    }

    Some(GapRow {
        start_ts: crate::time::format_ts(&prev_end_dt),
        end_ts: crate::time::format_ts(&next_start_dt),
        previous_project: prev.project_name.clone(),
    })
}

pub fn has_gap_between_trackings(
    prev: &db::Tracking,
    next: &db::Tracking,
    config: &Config,
) -> bool {
    build_gap_row(prev, next, config).is_some()
}

fn within_same_working_slice(
    start_local: chrono::DateTime<Local>,
    end_local: chrono::DateTime<Local>,
    config: &Config,
) -> bool {
    let weekday = start_local.weekday().num_days_from_monday() as u8;
    let start_min = start_local.hour() * 60 + start_local.minute();
    let end_min = end_local.hour() * 60 + end_local.minute();

    let ranges = config.working_hours.get(&weekday);
    if ranges.is_none() || ranges.is_some_and(|r| r.is_empty()) {
        return true;
    }

    for range in ranges.expect("checked is_some") {
        let Ok((sh, sm)) = parse_hhmm(&range.start) else {
            continue;
        };
        let Ok((eh, em)) = parse_hhmm(&range.end) else {
            continue;
        };
        let range_start = sh * 60 + sm;
        let range_end = eh * 60 + em;
        if start_min >= range_start
            && start_min <= range_end
            && end_min >= range_start
            && end_min <= range_end
        {
            return true;
        }
    }
    false
}
