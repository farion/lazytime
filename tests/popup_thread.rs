use lazytime::popup::{PopupAction, PopupRequest, ResumeAction, ResumePopupRequest};
#[cfg(not(feature = "popup-ui"))]
use lazytime::popup::{spawn_popup_thread, spawn_resume_popup_thread};
#[cfg(not(feature = "popup-ui"))]
use std::sync::mpsc;
#[cfg(not(feature = "popup-ui"))]
use std::time::Duration;

#[test]
#[cfg(not(feature = "popup-ui"))]
fn popup_thread_defaults_to_no_without_ui_feature() {
    let (tx, rx) = mpsc::channel();
    let handle = spawn_popup_thread(
        PopupRequest {
            output: None,
            message: "Start tracking?".to_string(),
        },
        tx,
    );

    let action = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("popup action");
    assert_eq!(action, PopupAction::No);
    handle.join().expect("join").expect("thread result");
}

#[test]
#[cfg(not(feature = "popup-ui"))]
fn resume_popup_thread_defaults_to_ignore_without_ui_feature() {
    let (tx, rx) = mpsc::channel();
    let handle = spawn_resume_popup_thread(
        ResumePopupRequest {
            output: None,
            project_name: "Alpha".to_string(),
            paused_tracking_id: 12,
            paused_at_ts: "2026-05-01T12:00:00Z".to_string(),
        },
        tx,
    );

    let action = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("resume popup action");
    assert_eq!(action, ResumeAction::Ignore);
    handle.join().expect("join").expect("thread result");
}

#[test]
#[cfg(feature = "popup-ui")]
fn popup_types_are_available_with_popup_ui_feature() {
    let _ = PopupRequest {
        output: Some("HDMI-A-1".to_string()),
        message: "Start tracking?".to_string(),
    };
    let _ = ResumePopupRequest {
        output: Some("HDMI-A-1".to_string()),
        project_name: "Alpha".to_string(),
        paused_tracking_id: 1,
        paused_at_ts: "2026-05-01T08:00:00Z".to_string(),
    };
    let _ = PopupAction::No;
    let _ = ResumeAction::Ignore;
}
