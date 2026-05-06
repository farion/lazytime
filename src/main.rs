use anyhow::{Context, Result};
use clap::Parser;
use lazytime::cli::CliArgs;
use lazytime::{config::Config, daemon, db, init_logging, jira_sync, time, tui};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();
    // Initialize logging with optional CLI override. If --loglevel is set, export it
    // to RUST_LOG so tracing_subscriber's EnvFilter picks it up.
    if !args.loglevel.trim().is_empty() {
        let lvl = args.loglevel.trim().to_uppercase();
        // std::env::set_var is considered unsafe in some toolchains; perform in unsafe block.
        unsafe { std::env::set_var("RUST_LOG", lvl) };
    }
    init_logging();
    let config = Config::from_path(args.config.as_deref())?;
    let conn = db::open(config.db_path())?;
    db::migrate(&conn)?;

    if args.daemon {
        return daemon::run_daemon(&config).await;
    }

    let mode_selected = args.daemon
        || args.tui
        || args.summary
        || args.report
        || args.jira_sync
        || args.waybar_state;

    if args.tui || !mode_selected {
        return tui::run(&config, args.config.as_deref());
    }

    if args.summary {
        run_summary(&config, args.watch).await?;
        return Ok(());
    }

    if args.report {
        run_report(&config, args.start.as_deref(), args.end.as_deref())?;
        return Ok(());
    }

    if args.jira_sync {
        run_jira_sync(&config, args.dry_run).await?;
        return Ok(());
    }

    if args.waybar_state {
        // Produce a single JSON object for waybar indicating current tracking state
        let conn = db::open(config.db_path())?;
        let active = db::get_active_tracking(&conn)?;
        // Compute today's total duration
        let rows = db::list_today(&conn)?;
        let mut today_secs: i64 = 0;
        for r in rows {
            let start = crate::time::parse_ts(&r.start_ts).ok();
            let end = r.end_ts.as_ref().and_then(|e| crate::time::parse_ts(e).ok()).or_else(|| Some(chrono::Utc::now()));
            if let (Some(s), Some(e)) = (start, end) {
                let secs = e.signed_duration_since(s).num_seconds();
                if secs > 0 {
                    today_secs += secs;
                }
            }
        }
        // Format durations as H:MM
        let fmt = |secs: i64| {
            if secs <= 0 {
                return "0:00".to_string();
            }
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            format!("{}:{:02}", h, m)
        };

        let text = if let Some(a) = active {
            // current tracking running
            let start = crate::time::parse_ts(&a.start_ts).ok();
            let cur_secs = if let Some(s) = start {
                let now = chrono::Utc::now();
                let secs = now.signed_duration_since(s).num_seconds();
                if secs > 0 { secs } else { 0 }
            } else { 0 };
            let cur_str = fmt(cur_secs);
            let today_str = fmt(today_secs);
            format!("{{\"text\":\"󱫠  {} {} | {}\",\"class\":\"active\"}}", a.project_name, cur_str, today_str)
        } else {
            let today_str = fmt(today_secs);
            format!("{{\"text\":\"No tracking  | {}\",\"class\":\"inactive\"}}", today_str)
        };
        println!("{}", text);
        return Ok(());
    }

    Ok(())
}

async fn run_summary(config: &Config, watch: bool) -> Result<()> {
    use crossterm::{cursor::MoveTo, terminal::Clear, terminal::ClearType};
    use std::io::stdout;
    let mut out = stdout();
    // Only use full-screen clear when attached to a tty; when piped, fall back to append behavior
    let is_tty = atty::is(atty::Stream::Stdout);
    loop {
        let conn = db::open(config.db_path())?;
        let rows = db::list_today(&conn)?;
        // Prepare pretty table: compute column widths and render aligned table
        let mut table_rows: Vec<(String, String, String, String, String, String)> = Vec::new();
        for row in rows {
            let start_formatted = match crate::time::parse_ts(&row.start_ts) {
                Ok(dt) => crate::time::format_ts(&dt),
                Err(_) => row.start_ts.clone(),
            };
            let end_formatted = match &row.end_ts {
                Some(e) => match crate::time::parse_ts(e) {
                    Ok(dt) => crate::time::format_ts(&dt),
                    Err(_) => e.clone(),
                },
                None => "(open)".to_string(),
            };
            // Calculate duration in seconds -> HH:mm
            let hours_str = {
                if let Ok(start_dt) = crate::time::parse_ts(&row.start_ts) {
                    let end_dt = match &row.end_ts {
                        Some(e) => crate::time::parse_ts(e).ok(),
                        None => Some(chrono::Utc::now()),
                    };
                    if let Some(ed) = end_dt {
                        let secs = ed.signed_duration_since(start_dt).num_seconds();
                        if secs > 0 {
                            let hrs = secs / 3600;
                            let mins = (secs % 3600) / 60;
                            format!("{}:{:02}", hrs, mins)
                        } else {
                            "00:00".to_string()
                        }
                    } else {
                        "-".to_string()
                    }
                } else {
                    "-".to_string()
                }
            };
            table_rows.push((
                row.id.to_string(),
                row.project_name.clone(),
                start_formatted,
                end_formatted,
                hours_str,
                row.jira_synced.to_string(),
            ));
        }

        // Column headers
        let h_id = "ID".to_string();
        let h_proj = "Project".to_string();
        let h_start = "Start".to_string();
        let h_end = "End".to_string();
        let h_syn = "Synced".to_string();

        let h_hours = "Hours".to_string();
        let mut w_id = h_id.len();
        let mut w_proj = h_proj.len();
        let mut w_start = h_start.len();
        let mut w_end = h_end.len();
        let mut w_hours = h_hours.len();
        let mut w_syn = h_syn.len();

        for (id, proj, start, end, hours, syn) in &table_rows {
            w_id = w_id.max(id.len());
            w_proj = w_proj.max(proj.len());
            w_start = w_start.max(start.len());
            w_end = w_end.max(end.len());
            w_hours = w_hours.max(hours.len());
            w_syn = w_syn.max(syn.len());
        }

        // Clear previous output and print header in-place when watching
        if watch {
            if is_tty {
                // execute is a macro; call it via execute!
                crossterm::execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;
            } else {
                // not a tty: print separator so logs remain readable
                println!("--- SUMMARY REFRESH ---");
            }
        }

        // Print header
        println!(
            "{id:>idw$} | {proj:<projw$} | {start:<startw$} | {end:<endw$} | {hours:>hoursw$} | {syn:>synw$}",
            id = h_id,
            proj = h_proj,
            start = h_start,
            end = h_end,
            hours = h_hours,
            syn = h_syn,
            idw = w_id,
            projw = w_proj,
            startw = w_start,
            endw = w_end,
            hoursw = w_hours,
            synw = w_syn
        );

        // Separator
        println!(
            "{:-<idw$}-+-{:-<projw$}-+-{:-<startw$}-+-{:-<endw$}-+-{:-<hoursw$}-+-{:-<synw$}",
            "",
            "",
            "",
            "",
            "",
            "",
            idw = w_id,
            projw = w_proj,
            startw = w_start,
            endw = w_end,
            hoursw = w_hours,
            synw = w_syn
        );

        // Rows
        for (id, proj, start, end, hours, syn) in table_rows {
            println!(
                "{id:>idw$} | {proj:<projw$} | {start:<startw$} | {end:<endw$} | {hours:>hoursw$} | {syn:>synw$}",
                id = id,
                proj = proj,
                start = start,
                end = end,
                hours = hours,
                syn = syn,
                idw = w_id,
                projw = w_proj,
                startw = w_start,
                endw = w_end,
                hoursw = w_hours,
                synw = w_syn
            );
        }

        if !watch {
            break;
        }
        sleep(Duration::from_secs(config.summary_update_seconds)).await;
        println!();
    }
    Ok(())
}

fn run_report(config: &Config, start: Option<&str>, end: Option<&str>) -> Result<()> {
    let start = start
        .map(|s| s.to_string())
        .or_else(|| config.report_start.clone())
        .context("report start missing. set --start or config.report_start")?;
    let end = end
        .map(|s| s.to_string())
        .or_else(|| config.report_end.clone())
        .context("report end missing. set --end or config.report_end")?;

    let conn = db::open(config.db_path())?;
    let rows = db::report_range(&conn, &start, &end)?;
    println!("Day | Project | Hours");
    println!("----|---------|------");
    for row in rows {
        let hours = row.seconds as f64 / 3600.0;
        println!("{} | {} | {:.2}", row.day, row.project_name, hours);
    }
    Ok(())
}

async fn run_jira_sync(config: &Config, dry_run: bool) -> Result<()> {
    jira_sync::run_jira_sync(config, dry_run, None).await
}
