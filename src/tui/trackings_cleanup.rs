use anyhow::Result;

use crate::config::Config;
use crate::db;
use crate::tui::trackings_rows::has_gap_between_trackings;

#[derive(Debug, Clone, Copy, Default)]
pub struct CleanupStats {
    pub merged_groups: usize,
    pub removed_rows: usize,
}

pub fn cleanup_today_unsynced_trackings(
    conn: &rusqlite::Connection,
    config: &Config,
) -> Result<CleanupStats> {
    let today = db::list_today(conn)?;
    let mut stats = CleanupStats::default();
    let mut group: Vec<db::Tracking> = Vec::new();

    let flush = |group: &mut Vec<db::Tracking>, stats: &mut CleanupStats| -> Result<()> {
        if group.len() <= 1 {
            group.clear();
            return Ok(());
        }
        let first = group[0].clone();
        let last_end = group.last().and_then(|t| t.end_ts.as_deref());
        db::update_tracking_times(
            conn,
            first.id,
            &first.project_name,
            &first.start_ts,
            last_end,
            first.notes.as_deref(),
        )?;
        for t in group.iter().skip(1) {
            db::delete_tracking(conn, t.id)?;
            stats.removed_rows += 1;
        }
        stats.merged_groups += 1;
        group.clear();
        Ok(())
    };

    for t in today {
        if t.jira_synced != 0 {
            flush(&mut group, &mut stats)?;
            continue;
        }

        if let Some(prev) = group.last() {
            let same_project = prev.project_name == t.project_name;
            let has_gap = has_gap_between_trackings(prev, &t, config);
            if !(same_project && !has_gap) {
                flush(&mut group, &mut stats)?;
            }
        }
        group.push(t);
    }
    flush(&mut group, &mut stats)?;

    Ok(stats)
}
