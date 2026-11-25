//! Looks up when the password expires and runs the original's timer rules (MECHANICS.md).

use crate::icon::Level;
use crate::settings::Settings;
use crate::winutil::{self, now_ft, TICKS_PER_DAY, TICKS_PER_MIN};
use std::os::windows::process::CommandExt;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const PRODUCT: &str = "Password Alert";
/// The original reuses a looked-up value for 5 minutes when more than 3 days are left.
const CACHE_MINUTES: i64 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Domain,
    Local,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    NotChecked,
    Never,
    Expires { days: i64, at_ft: i64, source: Source },
    Error(String),
}

pub struct Shared {
    pub status: Status,
    pub checking: bool,
    pub last_lookup_ft: Option<i64>,
    pub next_due: Option<Instant>,
    pub settings: Settings,
    /// Set by "Check now".
    pub force: bool,
    /// No pop-ups or tray (used by --screenshot).
    pub quiet: bool,
}

pub type State = Arc<(Mutex<Shared>, Condvar)>;

pub fn new_state(settings: Settings, quiet: bool) -> State {
    Arc::new((
        Mutex::new(Shared {
            status: Status::NotChecked,
            checking: false,
            last_lookup_ft: None,
            next_due: Some(Instant::now()), // first check immediately
            settings,
            force: false,
            quiet,
        }),
        Condvar::new(),
    ))
}

pub fn check_now(state: &State) {
    let (m, cv) = &**state;
    m.lock().unwrap().force = true;
    cv.notify_all();
}

/// Days left as the original counts them: whole days, truncated toward zero.
pub fn days_left(at_ft: i64, now: i64) -> i64 {
    (at_ft - now) / TICKS_PER_DAY
}

fn plural(d: i64) -> &'static str {
    if d == 1 { "day" } else { "days" }
}

pub fn message(status: &Status, s: &Settings) -> String {
    match status {
        Status::NotChecked => "Checking when your password expires…".into(),
        Status::Never => "Your password never expires.".into(),
        Status::Error(e) => format!("Couldn't read your password information: {e}"),
        Status::Expires { days, .. } if *days < 0 => {
            "Your password has expired. Please change it now.".into()
        }
        Status::Expires { days, .. } if *days <= s.warn_days => format!(
            "Your password will expire in {days} {}. Please change it from the Start menu",
            plural(*days)
        ),
        Status::Expires { days, .. } => {
            format!("Your password will expire in {days} {}.", plural(*days))
        }
    }
}

pub fn level(status: &Status, s: &Settings) -> Level {
    match status {
        Status::Expires { days, .. } if *days <= s.urgent_days => Level::Urgent,
        Status::Expires { days, .. } if *days <= s.warn_days => Level::Warn,
        Status::Expires { .. } | Status::Never => Level::Fine,
        _ => Level::Unknown,
    }
}

// ---- the lookup ----

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
try {
  Add-Type -AssemblyName System.DirectoryServices.AccountManagement
  $u = [System.DirectoryServices.AccountManagement.UserPrincipal]::Current
  if ($u.PasswordNeverExpires) { 'NEVER'; exit }
  $de = $u.GetUnderlyingObject()
  if ($de.Path -like 'LDAP://*') {
    $de.RefreshCache(@('msDS-UserPasswordExpiryTimeComputed'))
    $v = $de.Properties['msDS-UserPasswordExpiryTimeComputed'].Value
    $t = $v.GetType()
    $hi = [int64]$t.InvokeMember('HighPart', 'GetProperty', $null, $v, $null)
    $lo = [int64][uint32]$t.InvokeMember('LowPart', 'GetProperty', $null, $v, $null)
    'DOMAIN ' + (($hi -shl 32) + $lo)
  } else {
    $age = [int64]$de.Properties['PasswordAge'].Value
    $max = [int64]$de.Properties['MaxPasswordAge'].Value
    if ($max -le 0) { 'NEVER'; exit }
    'LOCAL ' + [DateTime]::UtcNow.AddSeconds($max - $age).ToFileTimeUtc()
  }
} catch { 'ERROR ' + $_.Exception.Message }
"#;

/// `--demo-days N` pretends the password expires in N days (to preview the warning states).
pub static DEMO_DAYS: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

fn lookup() -> Status {
    if let Some(d) = DEMO_DAYS.get() {
        let at = now_ft() + d * TICKS_PER_DAY + 3 * 60 * TICKS_PER_MIN;
        return Status::Expires { days: days_left(at, now_ft()), at_ft: at, source: Source::Domain };
    }
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .creation_flags(winutil::CREATE_NO_WINDOW)
        .output();
    let out = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(e) => return Status::Error(format!("couldn't start PowerShell ({e})")),
    };
    let line = out.lines().last().unwrap_or("").trim();
    let now = now_ft();
    if line == "NEVER" {
        return Status::Never;
    }
    let parse = |rest: &str, source| match rest.trim().parse::<i64>() {
        // 0x7FFF_FFFF_FFFF_FFFF is the directory's "never" value
        Ok(v) if v >= i64::MAX - 1 => Status::Never,
        Ok(v) => Status::Expires { days: days_left(v, now), at_ft: v, source },
        Err(_) => Status::Error("unexpected answer from the directory".into()),
    };
    if let Some(r) = line.strip_prefix("DOMAIN ") {
        return parse(r, Source::Domain);
    }
    if let Some(r) = line.strip_prefix("LOCAL ") {
        return parse(r, Source::Local);
    }
    let msg = line.strip_prefix("ERROR ").unwrap_or(line);
    Status::Error(if msg.is_empty() { "no answer".into() } else { msg.to_string() })
}

/// Callbacks into the tray, so the checker doesn't need to know about it.
pub type Notify = Box<dyn Fn(&Status, &Settings) + Send>;

/// The timer loop: `AlertPasswordExpiration` in the original.
pub fn run(state: State, on_change: Notify) {
    let (m, cv) = &*state;
    loop {
        // wait until due or forced
        let forced;
        {
            let mut g = m.lock().unwrap();
            loop {
                if g.force {
                    break;
                }
                match g.next_due {
                    Some(due) if Instant::now() >= due => break,
                    Some(due) => {
                        let wait = due.saturating_duration_since(Instant::now()).min(Duration::from_secs(30));
                        g = cv.wait_timeout(g, wait).unwrap().0;
                    }
                    None => g = cv.wait_timeout(g, Duration::from_secs(30)).unwrap().0,
                }
            }
            forced = g.force;
            g.force = false;

            // cache rule: reuse when more than `urgent_days` left and looked up ≤ 5 min ago
            let now = now_ft();
            let cached_ok = match (&g.status, g.last_lookup_ft) {
                (Status::Expires { days, .. }, Some(t)) => {
                    *days > g.settings.urgent_days && (now - t) <= CACHE_MINUTES * TICKS_PER_MIN
                }
                _ => false,
            };
            if cached_ok && !forced {
                if let Status::Expires { at_ft, source, .. } = g.status.clone() {
                    g.status = Status::Expires { days: days_left(at_ft, now), at_ft, source };
                }
            } else {
                g.checking = true;
                drop(g);
                let st = lookup();
                g = m.lock().unwrap();
                g.status = st;
                g.checking = false;
                g.last_lookup_ft = Some(now_ft());
            }

            // next interval: hourly when urgent, else daily (errors retry after the urgent interval)
            let s = g.settings.clone();
            let mins = match &g.status {
                Status::Expires { days, .. } if *days > s.urgent_days => s.normal_interval_min,
                Status::Never => s.normal_interval_min,
                _ => s.urgent_interval_min,
            };
            g.next_due = Some(Instant::now() + Duration::from_secs(mins.max(1) * 60));

            let status = g.status.clone();
            let quiet = g.quiet;
            drop(g);
            on_change(&status, &s);

            // the warning box when within the warning days (timer checks; "Check now" too)
            if !quiet && s.popups {
                if let Status::Expires { days, .. } = status {
                    if days <= s.warn_days {
                        winutil::warning_box(message(&status, &s), PRODUCT.into());
                    }
                }
            }
        }
    }
}
