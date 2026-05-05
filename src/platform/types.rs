#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub app_id: Option<String>,
    pub instance: Option<String>,
    pub class: Option<String>,
    pub title: String,
    pub workspace: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WindowEventInfo {
    pub app_id: Option<String>,
    pub instance: Option<String>,
    pub class: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, Copy)]
pub struct OutputRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum LockSource {
    ScreenSaver,
    Login1,
    Swaylock,
    WtsSession,
    MacosSession,
    Unknown,
}

impl LockSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScreenSaver => "ScreenSaver",
            Self::Login1 => "login1",
            Self::Swaylock => "swaylock",
            Self::WtsSession => "wts-session",
            Self::MacosSession => "macos-session",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LockEvent {
    Locked(LockSource),
    Unlocked(LockSource),
}
