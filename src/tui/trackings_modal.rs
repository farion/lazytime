use chrono::{Datelike, Timelike};

#[derive(Debug, Clone)]
pub enum TrackingsModal {
    Tracking(TrackingModal),
    Confirm(ConfirmModal),
    Filter(FilterModal),
}

#[derive(Debug, Clone)]
pub struct ConfirmModal {
    pub title: String,
    pub message: String,
    pub field_idx: usize,
    pub tracking_id: i64,
}

impl ConfirmModal {
    pub fn delete_tracking(tracking_id: i64) -> Self {
        Self {
            title: "Confirm delete".to_string(),
            message: "Delete selected tracking?".to_string(),
            field_idx: 1,
            tracking_id,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalMode {
    Add,
    Edit,
}

#[derive(Debug, Clone)]
    pub struct TrackingModal {
    pub mode: ModalMode,
    pub projects: Vec<String>,
    pub selected_project: Option<usize>,
    pub project_free_text: String,
    pub start: String,
    pub end: String,
    pub description: String,
    // whether tracking is synced to Jira (0/1 stored as text in modal for easy editing)
    pub synced: bool,
    pub field_idx: usize,
    pub date_picker: Option<DatePicker>,
    pub editing_id: Option<i64>,
}

impl TrackingModal {
    pub fn new_add_with_projects(projects: Vec<String>) -> Self {
        let selected = if projects.is_empty() { None } else { Some(0) };
        Self {
            mode: ModalMode::Add,
            projects,
            selected_project: selected,
            project_free_text: String::new(),
            start: String::new(),
            end: String::new(),
            description: String::new(),
            synced: false,
            field_idx: 0,
            date_picker: None,
            editing_id: None,
        }
    }

    pub fn new_edit_with_projects(
        id: i64,
        project_name: String,
        start: String,
        end: String,
        projects: Vec<String>,
    ) -> Self {
        let selected = if projects.is_empty() {
            None
        } else {
            projects.iter().position(|p| p == &project_name).or(Some(0))
        };
        Self {
            mode: ModalMode::Edit,
            projects,
            selected_project: selected,
            project_free_text: String::new(),
            start: format_storage_ts_for_tui(&start),
            end: format_storage_ts_for_tui(&end),
            description: String::new(),
            synced: false,
            field_idx: 0,
            date_picker: None,
            editing_id: Some(id),
        }
    }

    pub fn current_field_mut(&mut self) -> &mut String {
        match self.field_idx {
            0 => &mut self.project_free_text,
            1 => &mut self.start,
            2 => &mut self.end,
            3 => &mut self.description,
            _ => &mut self.project_free_text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterModal {
    pub start: String,
    pub end: String,
    pub field_idx: usize,
    pub date_picker: Option<DatePicker>,
}

impl FilterModal {
    pub fn new(start_day: String, end_day: String) -> Self {
        Self {
            start: format!("{} 00:00", start_day),
            end: format!("{} 23:59", end_day),
            field_idx: 0,
            date_picker: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatePicker {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub sel: usize,
}

impl DatePicker {
    pub fn from_str(s: &str) -> Self {
        use chrono::Local;
        let ndt = crate::time::parse_local_ts(s)
            .map(|dt| dt.with_timezone(&Local).naive_local())
            .unwrap_or_else(|_| Local::now().naive_local());

        Self {
            year: ndt.date().year(),
            month: ndt.date().month(),
            day: ndt.date().day(),
            hour: ndt.time().hour(),
            minute: ndt.time().minute(),
            sel: 0,
        }
    }

    pub fn format(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }

    pub fn format_with_focus(&self) -> String {
        let y = format!("{:04}", self.year);
        let m = format!("{:02}", self.month);
        let d = format!("{:02}", self.day);
        let h = format!("{:02}", self.hour);
        let min = format!("{:02}", self.minute);
        let (y, m, d, h, min) = match self.sel {
            0 => (format!("[{}]", y), m, d, h, min),
            1 => (y, format!("[{}]", m), d, h, min),
            2 => (y, m, format!("[{}]", d), h, min),
            3 => (y, m, d, format!("[{}]", h), min),
            4 => (y, m, d, h, format!("[{}]", min)),
            _ => (y, m, d, h, min),
        };
        format!("{}-{}-{} {}:{}", y, m, d, h, min)
    }

    pub fn inc(&mut self) {
        match self.sel {
            0 => self.year += 1,
            1 => {
                self.month = (self.month % 12) + 1;
            }
            2 => {
                let mdays = days_in_month(self.year, self.month);
                self.day = (self.day % mdays) + 1;
            }
            3 => {
                self.hour = (self.hour + 1) % 24;
            }
            4 => {
                self.minute = (self.minute + 1) % 60;
            }
            _ => {}
        }
    }

    pub fn dec(&mut self) {
        match self.sel {
            0 => self.year -= 1,
            1 => {
                self.month = if self.month == 1 { 12 } else { self.month - 1 };
            }
            2 => {
                let mdays = days_in_month(self.year, self.month);
                self.day = if self.day <= 1 { mdays } else { self.day - 1 };
            }
            3 => {
                self.hour = if self.hour == 0 { 23 } else { self.hour - 1 };
            }
            4 => {
                self.minute = if self.minute == 0 {
                    59
                } else {
                    self.minute - 1
                };
            }
            _ => {}
        }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn normalize_tui_ts(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    match crate::time::parse_local_ts(raw) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => raw.to_string(),
    }
}

pub fn format_storage_ts_for_tui(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    match crate::time::parse_ts(raw) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => raw.to_string(),
    }
}

pub fn normalize_storage_ts(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    if let Ok(dt) = crate::time::parse_local_ts(raw) {
        return crate::time::format_ts(&dt);
    }
    raw.to_string()
}
