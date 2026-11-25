//! Small Windows helpers: time, single instance, the Run key, message boxes, showing the window.

use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, FILETIME, HWND, SYSTEMTIME};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const WINDOW_TITLE: &str = "Password Alert";
pub const TICKS_PER_DAY: i64 = 864_000_000_000; // 100 ns ticks
pub const TICKS_PER_MIN: i64 = 600_000_000;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Now as a Windows FILETIME (100 ns ticks since 1601, UTC).
pub fn now_ft() -> i64 {
    let mut ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

fn local_st(ft: i64) -> Option<SYSTEMTIME> {
    let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut loc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe {
        if FileTimeToSystemTime(&f, &mut utc) == 0 {
            return None;
        }
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut loc) == 0 {
            return None;
        }
    }
    Some(loc)
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];

/// "Tuesday 8 December 2026 at 14:05" in local time.
pub fn format_date(ft: i64) -> String {
    match local_st(ft) {
        Some(s) => format!(
            "{} {} {} {} at {:02}:{:02}",
            DAYS[(s.wDayOfWeek as usize) % 7],
            s.wDay,
            MONTHS[(s.wMonth as usize).saturating_sub(1) % 12],
            s.wYear,
            s.wHour,
            s.wMinute
        ),
        None => "unknown date".into(),
    }
}

/// "14:05" in local time.
pub fn format_clock(ft: i64) -> String {
    match local_st(ft) {
        Some(s) => format!("{:02}:{:02}", s.wHour, s.wMinute),
        None => "--:--".into(),
    }
}

/// Returns false when another copy already holds the mutex.
pub fn single_instance() -> bool {
    let name = wide("Local\\PasswordAlert.Rebuild");
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        !(h.is_null() || GetLastError() == ERROR_ALREADY_EXISTS)
    }
}

// ---- the main window ----

pub static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
pub static WINDOW_SHOWN: AtomicBool = AtomicBool::new(true);
/// Set when the window couldn't be created; the tray then shows the message instead.
pub static NO_WINDOW: AtomicBool = AtomicBool::new(false);

fn find_window() -> HWND {
    let h = MAIN_HWND.load(Ordering::SeqCst);
    if h != 0 {
        return h as HWND;
    }
    let t = wide(WINDOW_TITLE);
    unsafe { FindWindowW(std::ptr::null(), t.as_ptr()) }
}

pub fn show_main_window() {
    let h = find_window();
    if !h.is_null() {
        unsafe {
            ShowWindow(h, SW_SHOW);
            ShowWindow(h, SW_RESTORE);
            SetForegroundWindow(h);
        }
        WINDOW_SHOWN.store(true, Ordering::SeqCst);
    }
}

pub fn hide_main_window() {
    let h = find_window();
    if !h.is_null() {
        unsafe { ShowWindow(h, SW_HIDE) };
        WINDOW_SHOWN.store(false, Ordering::SeqCst);
    }
}

// ---- warning box: one at a time ----

static BOX_OPEN: AtomicBool = AtomicBool::new(false);

pub fn warning_box(text: String, title: String) {
    if BOX_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let t = wide(&text);
        let c = wide(&title);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                t.as_ptr(),
                c.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_DEFAULT_DESKTOP_ONLY,
            );
        }
        BOX_OPEN.store(false, Ordering::SeqCst);
    });
}

// ---- change password ----

/// In a remote-desktop session, opens the Windows Security screen (what Ctrl+Alt+Del shows),
/// where "Change a password" lives. Returns false on a local session, where only Ctrl+Alt+Del can.
pub fn is_remote_session() -> bool {
    std::env::var("SESSIONNAME").map(|s| s.to_ascii_uppercase().starts_with("RDP")).unwrap_or(false)
}

/// For a local (non-domain) account on this computer: Settings → Sign-in options → Password.
pub fn open_signin_settings() -> bool {
    std::process::Command::new("explorer.exe").arg("ms-settings:signinoptions").spawn().is_ok()
}

pub fn open_change_password() -> bool {
    if !is_remote_session() {
        return false;
    }
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(New-Object -ComObject Shell.Application).WindowsSecurity()",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .is_ok()
}

// ---- start with Windows ----

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "PasswordAlert";

pub fn autostart_enabled() -> bool {
    run_value_exists(HKEY_CURRENT_USER)
}

/// Set by the deployment installer (Install.ps1) for every user of the server.
pub fn autostart_all_users() -> bool {
    run_value_exists(HKEY_LOCAL_MACHINE)
}

fn run_value_exists(root: HKEY) -> bool {
    let key = wide(RUN_KEY);
    let name = wide(RUN_NAME);
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(root, key.as_ptr(), 0, KEY_QUERY_VALUE, &mut hk) != 0 {
            return false;
        }
        let r = RegQueryValueExW(hk, name.as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        RegCloseKey(hk);
        r == 0
    }
}

pub fn set_autostart(on: bool) -> bool {
    let key = wide(RUN_KEY);
    let name = wide(RUN_NAME);
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_SET_VALUE, &mut hk) != 0 {
            return false;
        }
        let ok = if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let cmd = wide(&format!("\"{exe}\" --tray"));
            RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, cmd.as_ptr() as *const u8, (cmd.len() * 2) as u32) == 0
        } else {
            let r = RegDeleteValueW(hk, name.as_ptr());
            r == 0 || r == 2
        };
        RegCloseKey(hk);
        ok
    }
}

pub fn account_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(d) if !d.is_empty() => format!("{d}\\{user}"),
        _ => user,
    }
}
