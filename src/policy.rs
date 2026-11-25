//! Password requirements from Active Directory (or this computer's policy), checking a new
//! password against them, and changing it through Windows.

use crate::winutil::{self, wide};
use std::os::windows::process::CommandExt;

#[derive(Clone, Debug, Default)]
pub struct Requirements {
    /// Where the rules came from ("Domain policy", "Fine-grained policy …", "This computer's policy").
    pub source: String,
    pub min_length: u32,
    /// None when it couldn't be read (local accounts without admin rights).
    pub complexity: Option<bool>,
    pub history: u32,
    pub min_age_days: f64,
    pub max_age_days: f64,
    pub sam: String,
    pub display_name: String,
    pub domain_account: bool,
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
function LI($v) {
  if ($v -eq $null) { return 0 }
  if ($v -is [long] -or $v -is [int]) { return [int64]$v }
  $t = $v.GetType()
  return ([int64]$t.InvokeMember('HighPart','GetProperty',$null,$v,$null) -shl 32) + [int64][uint32]$t.InvokeMember('LowPart','GetProperty',$null,$v,$null)
}
try {
  Add-Type -AssemblyName System.DirectoryServices.AccountManagement
  $u = [System.DirectoryServices.AccountManagement.UserPrincipal]::Current
  'USER ' + $u.SamAccountName
  'NAME ' + $u.DisplayName
  $de = $u.GetUnderlyingObject()
  if ($de.Path -like 'LDAP://*') {
    'KIND domain'
    $done = $false
    try {
      $de.RefreshCache(@('msDS-ResultantPSO'))
      $pso = $de.Properties['msDS-ResultantPSO'].Value
      if ($pso) {
        $p = New-Object System.DirectoryServices.DirectoryEntry("LDAP://$pso")
        $len = $p.Properties['msDS-MinimumPasswordLength'].Value
        if ($len -ne $null) {
          'SOURCE Fine-grained policy ' + $p.Properties['cn'].Value
          'MIN ' + [int]$len
          'COMPLEX ' + [int][bool]$p.Properties['msDS-PasswordComplexityEnabled'].Value
          'HISTORY ' + [int]$p.Properties['msDS-PasswordHistoryLength'].Value
          'MINAGE ' + (LI $p.Properties['msDS-MinimumPasswordAge'].Value)
          'MAXAGE ' + (LI $p.Properties['msDS-MaximumPasswordAge'].Value)
          $done = $true
        }
      }
    } catch {}
    if (-not $done) {
      $root = New-Object System.DirectoryServices.DirectoryEntry('LDAP://RootDSE')
      $dn = $root.Properties['defaultNamingContext'].Value
      $d = New-Object System.DirectoryServices.DirectoryEntry("LDAP://$dn")
      'SOURCE Domain policy'
      'MIN ' + [int]$d.Properties['minPwdLength'].Value
      'COMPLEX ' + [int](([int]$d.Properties['pwdProperties'].Value -band 1) -ne 0)
      'HISTORY ' + [int]$d.Properties['pwdHistoryLength'].Value
      'MINAGE ' + (LI $d.Properties['minPwdAge'].Value)
      'MAXAGE ' + (LI $d.Properties['maxPwdAge'].Value)
    }
  } else {
    'KIND local'
    $c = New-Object System.DirectoryServices.DirectoryEntry("WinNT://$env:COMPUTERNAME,computer")
    'SOURCE This computer''s policy'
    'MIN ' + [int]$c.Properties['MinPasswordLength'].Value
    'HISTORY ' + [int]$c.Properties['PasswordHistoryLength'].Value
    'MINAGE_S ' + [int64]$c.Properties['MinPasswordAge'].Value
    'MAXAGE_S ' + [int64]$c.Properties['MaxPasswordAge'].Value
  }
} catch { 'ERROR ' + $_.Exception.Message }
"#;

/// AD stores these ages as negative 100 ns intervals; the "never" value is the most negative number.
fn ad_days(v: i64) -> f64 {
    if v == i64::MIN || v == 0 {
        0.0
    } else {
        (-(v as f64)) / winutil::TICKS_PER_DAY as f64
    }
}

pub fn fetch() -> Result<Requirements, String> {
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .creation_flags(winutil::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("couldn't start PowerShell ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut r = Requirements::default();
    let mut got_min = false;
    for line in text.lines().map(str::trim) {
        let (k, v) = line.split_once(' ').unwrap_or((line, ""));
        let num = || v.trim().parse::<i64>().unwrap_or(0);
        match k {
            "ERROR" => return Err(v.to_string()),
            "USER" => r.sam = v.to_string(),
            "NAME" => r.display_name = v.to_string(),
            "KIND" => r.domain_account = v == "domain",
            "SOURCE" => r.source = v.to_string(),
            "MIN" => {
                r.min_length = num().max(0) as u32;
                got_min = true;
            }
            "COMPLEX" => r.complexity = Some(num() != 0),
            "HISTORY" => r.history = num().max(0) as u32,
            "MINAGE" => r.min_age_days = ad_days(num()),
            "MAXAGE" => r.max_age_days = ad_days(num()),
            "MINAGE_S" => r.min_age_days = num().max(0) as f64 / 86400.0,
            "MAXAGE_S" => r.max_age_days = num().max(0) as f64 / 86400.0,
            _ => {}
        }
    }
    if !got_min {
        return Err("no answer from the directory".into());
    }
    Ok(r)
}

fn days_text(d: f64) -> String {
    if d < 1.0 {
        let h = (d * 24.0).round() as i64;
        format!("{h} {}", if h == 1 { "hour" } else { "hours" })
    } else {
        let n = d.round() as i64;
        format!("{n} {}", if n == 1 { "day" } else { "days" })
    }
}

impl Requirements {
    /// Plain-language list of the rules.
    pub fn lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.min_length > 0 {
            v.push(format!("At least {} characters", self.min_length));
        } else {
            v.push("No minimum length".into());
        }
        match self.complexity {
            Some(true) => {
                v.push("At least 3 of: uppercase letter, lowercase letter, number, symbol".into());
                v.push("Must not contain your user name or parts of your full name".into());
            }
            Some(false) => v.push("No character-type rules".into()),
            None => v.push("Character-type rules: couldn't be read on this computer".into()),
        }
        if self.history > 0 {
            v.push(format!("Can't be one of your last {} passwords", self.history));
        }
        if self.min_age_days > 0.0 {
            v.push(format!("Can be changed once every {}", days_text(self.min_age_days)));
        }
        if self.max_age_days > 0.0 {
            v.push(format!("Expires {} after it's set", days_text(self.max_age_days)));
        }
        v
    }

    /// Parts of the full name Windows forbids (3 characters or longer, split on , . - _ # tab space).
    fn name_tokens(&self) -> Vec<String> {
        self.display_name
            .split([',', '.', '-', '_', '#', '\t', ' '])
            .filter(|t| t.chars().count() >= 3)
            .map(|t| t.to_lowercase())
            .collect()
    }

    /// Each rule we can check before sending: (rule, passed).
    pub fn check(&self, new: &str, confirm: &str) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let len = new.chars().count() as u32;
        if self.min_length > 0 {
            let min = self.min_length;
            out.push((format!("At least {min} characters ({len} now)"), len >= min));
        } else {
            out.push(("Not empty".into(), len > 0));
        }
        if self.complexity != Some(false) {
            let upper = new.chars().any(|c| c.is_uppercase());
            let lower = new.chars().any(|c| c.is_lowercase());
            let digit = new.chars().any(|c| c.is_ascii_digit());
            let symbol = new.chars().any(|c| !c.is_alphanumeric());
            let other = new.chars().any(|c| c.is_alphabetic() && !c.is_uppercase() && !c.is_lowercase());
            let kinds = [upper, lower, digit, symbol, other].iter().filter(|b| **b).count();
            let tag = if self.complexity.is_none() { " (may apply)" } else { "" };
            out.push((format!("3 of: upper, lower, number, symbol ({kinds} now){tag}"), kinds >= 3));
            let low = new.to_lowercase();
            let sam_ok = self.sam.chars().count() < 3 || !low.contains(&self.sam.to_lowercase());
            let name_ok = self.name_tokens().iter().all(|t| !low.contains(t.as_str()));
            out.push((format!("Doesn't contain your user name or name{tag}"), new.is_empty() || (sam_ok && name_ok)));
        }
        out.push(("Both new passwords match".into(), !new.is_empty() && new == confirm));
        out
    }
}

/// Changes the password through Windows (NetUserChangePassword). Windows itself enforces the
/// history and minimum-age rules, which can't be checked beforehand.
pub fn change(domain_account: bool, user: &str, old: &str, new: &str) -> Result<(), String> {
    use windows_sys::Win32::NetworkManagement::NetManagement::NetUserChangePassword;
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let d = wide(&domain);
    let mut u = wide(user);
    let mut o = wide(old);
    let mut n = wide(new);
    let code = unsafe {
        NetUserChangePassword(
            if domain_account && !domain.is_empty() { d.as_ptr() } else { std::ptr::null() },
            u.as_ptr(),
            o.as_ptr(),
            n.as_ptr(),
        )
    };
    for buf in [&mut u, &mut o, &mut n] {
        buf.iter_mut().for_each(|c| *c = 0);
    }
    match code {
        0 => Ok(()),
        86 => Err("Your current password isn't right.".into()),
        2245 => Err("Windows refused the new password: it breaks a rule (length, character types, a recently used password, or it was changed too recently).".into()),
        5 => Err("Windows didn't allow the change (it may have been changed too recently).".into()),
        2221 => Err("Windows couldn't find your account.".into()),
        1311 | 1355 | 2453 => Err("Couldn't reach the domain. Check your network or VPN.".into()),
        c => Err(format!("Windows couldn't change the password (code {c}).")),
    }
}

/// Wipe a password string's memory before dropping it.
pub fn wipe(s: &mut String) {
    unsafe { s.as_bytes_mut().iter_mut().for_each(|b| *b = 0) };
    s.clear();
}
