use anyhow::Result;
use dbus::blocking::Connection as DbusConnection;
use dbus::message::MatchRule;
use std::sync::mpsc;

use super::types::{LockEvent, LockSource};

pub fn monitor_screensaver(tx_lock: mpsc::Sender<LockEvent>) -> Result<()> {
    let conn = DbusConnection::new_session()?;
    let rule = MatchRule::new_signal("org.freedesktop.ScreenSaver", "ActiveChanged");
    conn.add_match(rule, move |(active,): (bool,), _, _| {
        let event = if active {
            LockEvent::Locked(LockSource::ScreenSaver)
        } else {
            LockEvent::Unlocked(LockSource::ScreenSaver)
        };
        let _ = tx_lock.send(event);
        true
    })?;
    tracing::info!("lock monitor: listening for ScreenSaver ActiveChanged signals");
    loop {
        let _ = conn.process(std::time::Duration::from_millis(1000))?;
    }
}

pub fn monitor_login1(tx_lock: mpsc::Sender<LockEvent>) -> Result<()> {
    let conn = DbusConnection::new_system()?;
    let tx_sleep = tx_lock.clone();
    let sleep_rule = MatchRule::new_signal("org.freedesktop.login1.Manager", "PrepareForSleep");
    conn.add_match(sleep_rule, move |(going_to_sleep,): (bool,), _, _| {
        let event = if going_to_sleep {
            LockEvent::Locked(LockSource::Login1)
        } else {
            LockEvent::Unlocked(LockSource::Login1)
        };
        let _ = tx_sleep.send(event);
        true
    })?;

    let tx_lock_signal = tx_lock.clone();
    let lock_rule = MatchRule::new_signal("org.freedesktop.login1.Session", "Lock");
    conn.add_match(lock_rule, move |_: (), _, _| {
        let _ = tx_lock_signal.send(LockEvent::Locked(LockSource::Login1));
        true
    })?;

    let unlock_rule = MatchRule::new_signal("org.freedesktop.login1.Session", "Unlock");
    conn.add_match(unlock_rule, move |_: (), _, _| {
        let _ = tx_lock.send(LockEvent::Unlocked(LockSource::Login1));
        true
    })?;

    tracing::info!(
        "lock monitor: listening for login1 PrepareForSleep and Session Lock/Unlock signals"
    );
    loop {
        let _ = conn.process(std::time::Duration::from_millis(1000))?;
    }
}
