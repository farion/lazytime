use chrono::NaiveDate;

use crate::db;

const MAX_UNDO_STEPS_PER_DOMAIN: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UndoDomain {
    Trackings,
    VisualDay,
    Projects,
}

enum UndoSnapshot {
    TrackingsRange {
        start_date: String,
        end_date: String,
        rows: Vec<db::TrackingSnapshotRow>,
    },
    ProjectsAll {
        projects: Vec<db::Project>,
        rules: Vec<db::ProjectRule>,
    },
}

#[derive(Default)]
struct UndoStacks {
    trackings: Vec<UndoSnapshot>,
    visual_day: Vec<UndoSnapshot>,
    projects: Vec<UndoSnapshot>,
}

pub struct UndoState {
    stacks: UndoStacks,
}

impl Default for UndoState {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoState {
    pub fn new() -> Self {
        Self {
            stacks: UndoStacks::default(),
        }
    }

    pub fn remember_trackings_range(
        &mut self,
        conn: &rusqlite::Connection,
        filter_start: &str,
        filter_end: &str,
    ) -> anyhow::Result<()> {
        let (start, end) = normalize_date_range(filter_start, filter_end);
        let rows = db::snapshot_trackings_for_range(conn, &start, &end)?;
        push_limited(
            &mut self.stacks.trackings,
            UndoSnapshot::TrackingsRange {
            start_date: start,
            end_date: end,
            rows,
            },
        );
        Ok(())
    }

    pub fn remember_visual_day(
        &mut self,
        conn: &rusqlite::Connection,
        day: NaiveDate,
    ) -> anyhow::Result<()> {
        let date = day.format("%Y-%m-%d").to_string();
        let rows = db::snapshot_trackings_for_range(conn, &date, &date)?;
        push_limited(
            &mut self.stacks.visual_day,
            UndoSnapshot::TrackingsRange {
                start_date: date.clone(),
                end_date: date,
                rows,
            },
        );
        Ok(())
    }

    pub fn remember_projects(&mut self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        let projects = db::projects(conn)?;
        let rules = db::list_all_rules(conn)?;
        push_limited(
            &mut self.stacks.projects,
            UndoSnapshot::ProjectsAll { projects, rules },
        );
        Ok(())
    }

    pub fn undo(
        &mut self,
        conn: &mut rusqlite::Connection,
        domain: UndoDomain,
    ) -> anyhow::Result<Option<String>> {
        let snapshot = match domain {
            UndoDomain::Trackings => self.stacks.trackings.pop(),
            UndoDomain::VisualDay => self.stacks.visual_day.pop(),
            UndoDomain::Projects => self.stacks.projects.pop(),
        };

        let Some(snapshot) = snapshot else {
            return Ok(None);
        };

        match (domain, snapshot) {
            (
                UndoDomain::Trackings | UndoDomain::VisualDay,
                UndoSnapshot::TrackingsRange {
                    start_date,
                    end_date,
                    rows,
                },
            ) => {
                db::restore_trackings_for_range(conn, &start_date, &end_date, &rows)?;
                Ok(Some("undo applied".to_string()))
            }
            (UndoDomain::Projects, UndoSnapshot::ProjectsAll { projects, rules }) => {
                db::restore_projects_and_rules(conn, &projects, &rules)?;
                Ok(Some("undo applied".to_string()))
            }
            _ => Ok(None),
        }
    }
}

fn normalize_date_range(filter_start: &str, filter_end: &str) -> (String, String) {
    let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let today = chrono::Local::now().date_naive();
    let sday = parse_day(filter_start).unwrap_or(today);
    let eday = parse_day(filter_end).unwrap_or(sday);
    if sday <= eday {
        (
            sday.format("%Y-%m-%d").to_string(),
            eday.format("%Y-%m-%d").to_string(),
        )
    } else {
        (
            eday.format("%Y-%m-%d").to_string(),
            sday.format("%Y-%m-%d").to_string(),
        )
    }
}

fn push_limited(stack: &mut Vec<UndoSnapshot>, snapshot: UndoSnapshot) {
    if stack.len() >= MAX_UNDO_STEPS_PER_DOMAIN {
        let overflow = stack.len() + 1 - MAX_UNDO_STEPS_PER_DOMAIN;
        stack.drain(..overflow);
    }
    stack.push(snapshot);
}
