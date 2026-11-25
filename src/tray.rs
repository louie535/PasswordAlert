//! The tray icon: tooltip, balloon on click (30 s, as the original), right-click menu.

use crate::checker::{self, State, PRODUCT};
use crate::icon::{self, Level};
use crate::winutil::{self, wide};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const WM_UPDATE: u32 = WM_APP + 2;
const WM_BALLOON: u32 = WM_APP + 3;
const WM_QUIT_APP: u32 = WM_APP + 4;
const ID_OPEN: usize = 1;
const ID_CHECK: usize = 2;
const ID_EXIT: usize = 3;
/// The original's balloon timeout (IconClick, 30000 ms).
const BALLOON_MS: u32 = 30_000;

static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicIsize = AtomicIsize::new(0);
static STATE: OnceLock<State> = OnceLock::new();
static CURRENT: Mutex<(String, Option<Level>)> = Mutex::new((String::new(), None));
static ICONS: OnceLock<[isize; 4]> = OnceLock::new();
pub static REPAINT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

fn level_index(l: Level) -> usize {
    match l {
        Level::Fine => 0,
        Level::Warn => 1,
        Level::Urgent => 2,
        Level::Unknown => 3,
    }
}

fn make_icon(level: Level) -> isize {
    let size = 32u32;
    let rgba = icon::rgba(size, level);
    let mut bgra = Vec::with_capacity(rgba.len());
    for p in rgba.chunks(4) {
        bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    let mask = vec![0u8; (size * size / 8) as usize];
    unsafe {
        CreateIcon(GetModuleHandleW(std::ptr::null()), size as i32, size as i32, 1, 32, mask.as_ptr(), bgra.as_ptr()) as isize
    }
}

fn copy_into(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = 1;
    d
}

fn add_icon(hwnd: HWND) {
    let (text, level) = CURRENT.lock().unwrap().clone();
    let mut d = base_data(hwnd);
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = ICONS.get().unwrap()[level_index(level.unwrap_or(Level::Unknown))] as HICON;
    copy_into(&mut d.szTip, &tip(&text));
    unsafe { Shell_NotifyIconW(NIM_ADD, &d) };
}

fn tip(text: &str) -> String {
    if text.is_empty() {
        PRODUCT.to_string()
    } else {
        let t = format!("{PRODUCT}\n{text}");
        t.chars().take(127).collect()
    }
}

fn update_icon(hwnd: HWND) {
    let (text, level) = CURRENT.lock().unwrap().clone();
    let mut d = base_data(hwnd);
    d.uFlags = NIF_ICON | NIF_TIP;
    d.hIcon = ICONS.get().unwrap()[level_index(level.unwrap_or(Level::Unknown))] as HICON;
    copy_into(&mut d.szTip, &tip(&text));
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
}

fn balloon(hwnd: HWND) {
    let (text, level) = CURRENT.lock().unwrap().clone();
    let text = if text.is_empty() { PRODUCT.to_string() } else { text };
    let mut d = base_data(hwnd);
    d.uFlags = NIF_INFO;
    d.Anonymous.uTimeout = BALLOON_MS;
    d.dwInfoFlags = match level {
        Some(Level::Warn) | Some(Level::Urgent) => NIIF_WARNING,
        _ => NIIF_INFO,
    };
    copy_into(&mut d.szInfo, &text);
    copy_into(&mut d.szInfoTitle, PRODUCT);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
}

fn open_window() {
    if winutil::NO_WINDOW.load(Ordering::SeqCst) {
        let h = TRAY_HWND.load(Ordering::SeqCst);
        balloon(h as HWND);
        return;
    }
    winutil::show_main_window();
    if let Some(r) = REPAINT.get() {
        r();
    }
}

fn menu(hwnd: HWND) {
    unsafe {
        let m = CreatePopupMenu();
        let open = wide("Open Password Alert");
        let check = wide("Check now");
        let exit = wide("Exit");
        AppendMenuW(m, MF_STRING, ID_OPEN, open.as_ptr());
        AppendMenuW(m, MF_STRING, ID_CHECK, check.as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(m, MF_STRING, ID_EXIT, exit.as_ptr());
        let mut pt = std::mem::zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
        DestroyMenu(m);
        match cmd as usize {
            ID_OPEN => open_window(),
            ID_CHECK => {
                if let Some(s) = STATE.get() {
                    checker::check_now(s);
                }
            }
            ID_EXIT => quit(hwnd),
            _ => {}
        }
    }
}

fn quit(hwnd: HWND) {
    let d = base_data(hwnd);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
    std::process::exit(0);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg != 0 && msg as isize == TASKBAR_CREATED.load(Ordering::SeqCst) {
        add_icon(hwnd); // Explorer restarted
        return 0;
    }
    match msg {
        WM_TRAY => {
            match lp as u32 {
                WM_LBUTTONUP => balloon(hwnd),
                WM_LBUTTONDBLCLK => open_window(),
                WM_RBUTTONUP | WM_CONTEXTMENU => menu(hwnd),
                _ => {}
            }
            0
        }
        WM_UPDATE => {
            update_icon(hwnd);
            0
        }
        WM_BALLOON => {
            balloon(hwnd);
            0
        }
        WM_QUIT_APP => {
            quit(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Called by the checker after every check.
pub fn set_status(text: String, level: Level) {
    *CURRENT.lock().unwrap() = (text, Some(level));
    let h = TRAY_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_UPDATE, 0, 0) };
    }
}

pub fn show_balloon() {
    let h = TRAY_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_BALLOON, 0, 0) };
    }
}

pub fn exit_app() {
    let h = TRAY_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, WM_QUIT_APP, 0, 0) };
    } else {
        std::process::exit(0);
    }
}

/// Runs the tray on its own thread with its own message loop.
pub fn spawn(state: State) {
    let _ = STATE.set(state);
    std::thread::spawn(|| unsafe {
        let _ = ICONS.set([
            make_icon(Level::Fine),
            make_icon(Level::Warn),
            make_icon(Level::Urgent),
            make_icon(Level::Unknown),
        ]);
        let class = wide("PasswordAlertTray");
        let tc = wide("TaskbarCreated");
        TASKBAR_CREATED.store(RegisterWindowMessageW(tc.as_ptr()) as isize, Ordering::SeqCst);
        let hinst = GetModuleHandleW(std::ptr::null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        TRAY_HWND.store(hwnd as isize, Ordering::SeqCst);
        add_icon(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
