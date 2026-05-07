use anyhow::Result;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};

use crate::config::{Config, parse_hhmm};
use crate::db;
use crate::platform::types::{WindowEventInfo, WindowInfo};
use crate::rules::RuleCache;

const MANUAL_STOP_SNOOZE_UNTIL_KEY: &str = "autotracking_snooze_until";
pub const AUTOTRACKING_SUSPENDED_UNTIL_KEY: &str = "autotracking_suspended_until";

#[derive(Debug, Clone)]
pub struct DaemonState {
    config: Config,
    reminder_next_at: Option<DateTime<Utc>>,
    reminder_popup_open: bool,
    autotracking_suspended: bool,
    paused: Option<PausedTracking>,
    last_output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PausedTracking {
    pub id: i64,
    pub project_name: String,
    pub start_ts: String,
    pub paused_at: DateTime<Utc>,
    pub output: Option<String>,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            reminder_next_at: None,
            reminder_popup_open: false,
            autotracking_suspended: false,
            paused: None,
            last_output: None,
        }
    }

    pub async fn process_event(
        &mut self,
        conn: &rusqlite::Connection,
        rules: &RuleCache,
        info: WindowInfo,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.last_output = info.output.clone();

        let ruleset = rules.get().await;
        let detected = ruleset
            .detect_project(&WindowEventInfo {
                app_id: info.app_id.clone(),
                instance: info.instance.clone(),
                class: info.class.clone(),
                title: info.title.clone(),
            })
            .unwrap_or_else(|| self.config.default_project.clone());

        let active = db::get_active_tracking(conn)?;
        self.refresh_autotracking_suspension(conn, now)?;
        if self.paused.is_some() {
            tracing::debug!("tracking is paused due to lock; skipping daemon auto-tracking");
            return Ok(());
        }
        if active.is_none() && self.manual_stop_snooze_active(conn, now)? {
            return Ok(());
        }
        if active.is_none() && (self.reminder_popup_open || self.autotracking_suspended) {
            tracing::debug!(
                "autotracking suspended or reminder popup open; skipping daemon auto-start"
            );
            return Ok(());
        }
        if active.is_none() && !self.within_working_hours(now) {
            tracing::debug!("outside configured working hours; skipping daemon auto-start");
            return Ok(());
        }
        if active.is_none() {
            let mut conn = db::open(self.config.db_path())?;
            db::start_tracking(
                &mut conn,
                &detected,
                "daemon",
                info.app_id.as_deref(),
                info.instance.as_deref(),
                Some(&info.title),
                info.workspace.as_deref(),
                info.output.as_deref(),
                now,
            )?;
            tracing::info!(
                "started tracking project '{}' (app_id={:?} instance={:?} class={:?} title='{}')",
                detected,
                info.app_id,
                info.instance,
                info.class,
                info.title
            );
            return Ok(());
        }

        let active = active.expect("checked is_some");
        if active.project_name == detected {
            return Ok(());
        }

        // If detection resolved to the configured default project, do not auto-switch
        // away from an actively running tracking to the default. The default project
        // is intended as a fallback when nothing else matches, and should not replace
        // a running user tracking automatically.
        if detected == self.config.default_project {
            tracing::debug!(
                "detected default project but active tracking exists; skipping auto-switch"
            );
            return Ok(());
        }

        // active.start_ts is stored in our format YYYY-MM-ddTHH:mm:ss (UTC)
        let elapsed_since_last_change = crate::time::parse_ts(&active.start_ts)
            .map(|dt| now.signed_duration_since(dt).num_seconds())
            .unwrap_or(0);

        // If the title/window context changed and the previous change is older than
        // tracking_stability_seconds, switch immediately on this event.
        if elapsed_since_last_change >= self.config.tracking_stability_seconds as i64 {
            let mut conn = db::open(self.config.db_path())?;
            db::switch_tracking(
                &mut conn,
                &detected,
                info.app_id.as_deref(),
                info.instance.as_deref(),
                Some(&info.title),
                info.workspace.as_deref(),
                info.output.as_deref(),
                now,
            )?;
            tracing::info!(
                "switched tracking to project '{}' (app_id={:?} instance={:?} class={:?} title='{}')",
                detected,
                info.app_id,
                info.instance,
                info.class,
                info.title
            );
        }

        Ok(())
    }

    pub fn manual_stop_snooze_active(
        &self,
        conn: &rusqlite::Connection,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let Some(raw_until) = db::get_config_key(conn, MANUAL_STOP_SNOOZE_UNTIL_KEY)? else {
            return Ok(false);
        };
        let Ok(until) = crate::time::parse_ts(&raw_until) else {
            return Ok(false);
        };
        if now < until {
            tracing::debug!(
                "manual stop snooze active; skipping auto-tracking until {}",
                crate::time::format_ts_local(&until)
            );
            return Ok(true);
        }
        Ok(false)
    }

    pub fn reminder_due(&self, now: DateTime<Utc>) -> bool {
        if self.autotracking_suspended {
            return false;
        }
        if !self.within_working_hours(now) {
            return false;
        }
        match self.reminder_next_at {
            Some(ts) => now >= ts,
            None => true,
        }
    }

    pub fn reminder_no(&mut self, conn: &rusqlite::Connection, now: DateTime<Utc>) -> Result<()> {
        self.autotracking_suspended = true;
        let next_at = now + chrono::Duration::seconds(self.config.track_reminder_seconds as i64);
        db::upsert_config_key(
            conn,
            AUTOTRACKING_SUSPENDED_UNTIL_KEY,
            &crate::time::format_ts(&next_at),
        )?;
        self.reminder_next_at = Some(next_at);
        tracing::info!(
            "autotracking_paused: reason=reminder_no next_reminder_at={}",
            crate::time::format_ts_local(&next_at)
        );
        Ok(())
    }

    pub fn reminder_snooze(
        &mut self,
        conn: &rusqlite::Connection,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.autotracking_suspended = true;
        let next_at =
            now + chrono::Duration::seconds(self.config.track_reminder_snooze_seconds as i64);
        db::upsert_config_key(
            conn,
            AUTOTRACKING_SUSPENDED_UNTIL_KEY,
            &crate::time::format_ts(&next_at),
        )?;
        self.reminder_next_at = Some(next_at);
        tracing::info!(
            "autotracking_paused: reason=reminder_snooze next_reminder_at={}",
            crate::time::format_ts_local(&next_at)
        );
        Ok(())
    }

    pub fn mark_popup_shown(&mut self, now: DateTime<Utc>) {
        self.reminder_popup_open = true;
        self.reminder_next_at = Some(now + chrono::Duration::seconds(30));
    }

    pub fn clear_reminder_popup(&mut self) {
        self.reminder_popup_open = false;
    }

    pub fn reminder_popup_open(&self) -> bool {
        self.reminder_popup_open
    }

    pub fn resume_autotracking(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        self.autotracking_suspended = false;
        db::release_lock(conn, AUTOTRACKING_SUSPENDED_UNTIL_KEY)?;
        Ok(())
    }

    pub fn autotracking_suspended(&self) -> bool {
        self.autotracking_suspended
    }

    pub fn refresh_autotracking_suspension(
        &mut self,
        conn: &rusqlite::Connection,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Some(raw_until) = db::get_config_key(conn, AUTOTRACKING_SUSPENDED_UNTIL_KEY)? else {
            self.autotracking_suspended = false;
            return Ok(());
        };

        let Ok(until) = crate::time::parse_ts(&raw_until) else {
            self.autotracking_suspended = false;
            db::release_lock(conn, AUTOTRACKING_SUSPENDED_UNTIL_KEY)?;
            return Ok(());
        };

        if now < until {
            self.autotracking_suspended = true;
            self.reminder_next_at = Some(until);
            return Ok(());
        }

        self.autotracking_suspended = false;
        db::release_lock(conn, AUTOTRACKING_SUSPENDED_UNTIL_KEY)?;
        tracing::info!(
            "autotracking resumed automatically after suspension window elapsed at={}",
            crate::time::format_ts_local(&now)
        );
        Ok(())
    }

    pub fn mark_paused(&mut self, paused: PausedTracking) {
        tracing::info!(
            "tracking_paused: id={} project={} start_ts={} paused_at={} output={:?}",
            paused.id,
            paused.project_name,
            paused.start_ts,
            crate::time::format_ts(&paused.paused_at),
            paused.output
        );
        self.paused = Some(paused);
    }

    pub fn paused(&self) -> Option<&PausedTracking> {
        self.paused.as_ref()
    }

    pub fn take_paused(&mut self) -> Option<PausedTracking> {
        let paused = self.paused.take();
        if let Some(ref item) = paused {
            tracing::info!(
                "tracking_paused_take: id={} project={} paused_at={}",
                item.id,
                item.project_name,
                crate::time::format_ts(&item.paused_at)
            );
        }
        paused
    }

    pub fn last_output(&self) -> Option<&str> {
        self.last_output.as_deref()
    }

    fn within_working_hours(&self, now: DateTime<Utc>) -> bool {
        let local_now = now.with_timezone(&Local);
        let weekday = local_now.weekday().num_days_from_monday() as u8;
        let ranges = match self.config.working_hours.get(&weekday) {
            Some(v) => v,
            None => return false,
        };
        let current_minutes = local_now.hour() * 60 + local_now.minute();
        for range in ranges {
            let Ok((sh, sm)) = parse_hhmm(&range.start) else {
                continue;
            };
            let Ok((eh, em)) = parse_hhmm(&range.end) else {
                continue;
            };
            let start = sh * 60 + sm;
            let end = eh * 60 + em;
            if current_minutes >= start && current_minutes <= end {
                return true;
            }
        }
        false
    }
}
