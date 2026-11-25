# Password Alert — rules sheet

Read from the original's code. Method tokens (0x06…) are the "address" of each managed rule; native addresses (0x1400…) are in PasswordAlert.exe.

## The main program — PasswordAlert.exe (native decompile, Ghidra)
PasswordAlert.exe is Microsoft's stock .NET Core **3.1.18** launcher ("apphost"); it holds none of the alert logic.
- `0x140011d00` (FUN_140011d00): resolves its own path, finds `hostfxr.dll`, prefers `hostfxr_main_startupinfo`
  (falls back to `hostfxr_main`), then hands control to the .NET runtime. Evidence `ev_539d6497…`.
- `0x1400124f0` (FUN_1400124f0): reads the bound managed program name, `PasswordAlert.dll` (string at `0x1400290f0`),
  and refuses to run if the placeholder hash is still there. Evidence `ev_516d1341…`.
- `0x14000b150` (FUN_14000b150): the exe's only `MessageBoxW` — "To run this application, you must install .NET Core…
  Would you like to download it now?" (Yes/No, opens the download page). It isn't the password warning. Evidence `ev_40aa921e…`.
- None of the alert strings (`msDS-UserPasswordExpiryTimeComputed`, the messages, the mutex name) are in the exe
  (`native_strings_find` `ev_feab9f20…`, `ev_deb7940a…`). The rebuild is a single native program, so it needs no launcher.

## The alert logic — PasswordAlert.dll (.NET Core 3.1 WinForms, IL read instruction by instruction)
Evidence: managed_members_list `ev_02ee6966…` (AlertContext) and `ev_00490220…` (Program, PasswordInformation, AdsDate).
Every number below is an IL literal (`ldc.i4` / `ldc.r8`) at the named method.

## Start-up — `Program::Main` (0x0600000d)
- Single instance: named mutex `Local\com.ascentis.PasswordAlert`; `WaitOne(0)` fails → exit silently.
  Rebuild: mutex `Local\PasswordAlert.Rebuild`; a second start brings the running window forward instead of doing nothing.
- No window: only a tray (notify) icon. Tooltip and balloon title = product name.

## Tray icon — `AlertContext::.ctor` (0x06000006), `IconClick` (0x06000007)
- Tooltip text = product name; visible at start.
- Click on the icon → balloon tip with the current message, timeout **30000 ms**.
- On exit the icon is hidden (`ThreadExit` → `Visible = false`).

## Timer — `AlertContext::.ctor`, `AlertPasswordExpiration` (0x06000008)
- First check immediately (due time `TimeSpan.Zero`), period **60 minutes**.
- On every check: `days = PasswordExpirationDays()`.
  - `days <= 3` → next interval **60 minutes**; otherwise **1 day** (`Timer.Change(interval, interval)`).
  - Balloon text is refreshed.
  - `days <= 10` → warning message box (OK button, warning icon, `MessageBoxOptions` 0x20000 = default-desktop-only) with the message and product name as title.

## Message text — `get_BalloonTipText` (0x06000009)
- `days <= 10` → "Your … password will expire in {0} days. Please change it from the Start menu"
- otherwise → "Your … password will expire in {0} days."
- Rebuild uses neutral wording without the company name.

## Days left and cache — `PasswordExpirationDays` (0x0600000a)
- If the cached value is **> 3** and the last lookup was **≤ 5 minutes** ago, the cached value is reused.
- Otherwise look up again and remember the time.

## Lookup — `PasswordInformation::get_PasswordExpirationDays` (0x0600000e), `AdsDate::ToDateTime` (0x06000017)
- `UserPrincipal.Current.PasswordNeverExpires` → `int.MaxValue` days.
- Else read the directory attribute `msDS-UserPasswordExpiryTimeComputed` (a large integer: `HighPart << 32 + LowPart`),
  convert with `DateTime.FromFileTimeUtc`, `days = (expiry − UtcNow).Days` (whole days, truncated toward zero).

## Changes in the rebuild (the person's "additional changes")
- A status window: days left, expiry date and time, account kind, last/next check, colour by urgency.
- **Check now** button (always does a fresh lookup, skipping the 5-minute cache).
- **Change password** button: in a remote-desktop session opens the Windows Security screen (Start → Windows Security);
  on a local session it tells the person to press Ctrl+Alt+Del (Windows doesn't let programs open that screen locally).
- **Change password page** (added on request; not in the original):
  - Rules come from AD: the user's fine-grained policy (`msDS-ResultantPSO` → `msDS-MinimumPasswordLength`,
    `msDS-PasswordComplexityEnabled`, `msDS-PasswordHistoryLength`, `msDS-Minimum/MaximumPasswordAge`) when readable,
    else the domain head (`minPwdLength`, `pwdProperties` bit 1 = complexity, `pwdHistoryLength`, `minPwdAge`, `maxPwdAge`;
    ages are negative 100 ns intervals). Local accounts: `WinNT://<computer>` MinPasswordLength, PasswordHistoryLength, ages in seconds.
  - Complexity check follows Windows' rule: 3 of upper / lower / digit / symbol (other letters count as a fifth kind),
    and no sAMAccountName (3+ chars) or full-name parts of 3+ chars split on `, . - _ #` tab and space.
  - "Open Windows change password": remote session → Windows Security; local account → Settings sign-in options;
    domain account on a local console → Ctrl+Alt+Del instructions (Windows doesn't let apps open that screen).
  - The in-app form changes the password with Windows' `NetUserChangePassword`, so Windows enforces history and minimum age.
  - `--change-page` opens straight on this page (used for pictures).
- `--demo-days N` start option pretends the password expires in N days, to preview the warning states.
- Adjustable: warning days (default 10), hourly-check days (default 3), pop-up on/off.
- **Start with Windows** (current user's Run key, starts hidden in the tray with `--tray`).
- **Show test warning** button.
- Tray menu (right-click): Open, Check now, Exit. Double-click opens the window. Left click shows the balloon as before.
- Tray icon colour follows urgency (blue = fine, amber = warning days, red = hourly days/expired).
- Local (non-domain) accounts: expiry estimated from the account's password age and maximum age, since they have no computed attribute.
- "Never expires" and "has expired" get their own wording instead of "2147483647 days" / negative days.
- Only one warning box at a time (the original could stack boxes if one was left open).
- A failed lookup shows an error and retries after 60 minutes (the original would crash on an exception in the timer).
