# Password Alert

A small Windows tray app that warns users before their Windows / Active Directory password expires.

- Checks once a day, and every hour once **3 days or fewer** are left.
- Pops up a warning when **10 days or fewer** are left (both adjustable per user).
- Window with days left, the exact expiry date, **Check now**, and a **Change password** page that reads the
  password rules from Active Directory, checks a new password against them as you type, and changes it through Windows.
- Starts minimized to the tray; optional "Start with Windows"; single instance.

The timing rules match the original tool; see [MECHANICS.md](MECHANICS.md) for every value and where it came from.

## Requirements

- 64-bit Windows 10 / Windows Server 2016 or newer.
- To build: [Rust](https://rustup.rs) (rustup). Nothing else: no Visual Studio needed.

## Build

```powershell
cargo build --release
```

The program is `target\release\password-alert.exe`, a single file with no runtime to install.

Notes:
- `rust-toolchain.toml` selects the **GNU** toolchain (`stable-x86_64-pc-windows-gnu`); rustup installs it
  automatically. This avoids needing the Visual C++ linker.
- `build.rs` supplies an import library the GNU toolchain lacks (`shlwapi`), taken from the `windows_x86_64_gnu` crate.
- `Cargo.lock` pins a few crates (parking_lot, parking_lot_core, libloading, arboard, webbrowser) to versions that
  don't need `dlltool`. Avoid a blanket `cargo update`; update crates one at a time and rebuild.

### Start options

| Option | What it does |
| --- | --- |
| *(none)* | Start; minimized to the tray unless the user turned that off |
| `--show` | Start with the window open |
| `--tray` | Start hidden in the tray (used at sign-in) |
| `--screenshot <file.png>` | Draw the window, save a picture, exit (used by automated checks) |
| `--demo-days <n>` | Pretend the password expires in *n* days (preview the warning states) |
| `--change-page` | Open on the Change password page |

Starting it again while it runs brings the running copy's window forward.

## Package for deployment

```powershell
powershell -ExecutionPolicy Bypass -File package.ps1
```

Builds the release program and writes `dist\PasswordAlert-<version>.zip` containing `PasswordAlert.exe`,
`Install.cmd`, `Install.ps1`, `Uninstall.ps1` and `README.txt` (from the `deploy\` folder).

## Install on a server (all users)

1. Copy the unzipped `PasswordAlert` folder to the server.
2. Right-click `Install.cmd` → **Run as administrator**
   (or `powershell -ExecutionPolicy Bypass -File Install.ps1`).
   - Copies the program to `C:\Program Files\PasswordAlert`
   - Starts it in the tray at every user's sign-in (`HKLM\...\Run`, `--tray`)
   - Adds **Password Alert** to the Start menu
   - Options: `-InstallDir <folder>`, `-NoAutoStart`, `-NoShortcut`
3. Users get it at their next sign-in. Running the installer again upgrades in place.

Silent deployment (GPO startup script, SCCM/Intune, PDQ…):

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File Install.ps1
```

Remove with `Uninstall.ps1` (run as administrator). Per-user settings live in `%APPDATA%\PasswordAlert\settings.json`.

## How it works

| Part | File |
| --- | --- |
| Window and settings UI | `src/main.rs` |
| Expiry lookup and timer rules | `src/checker.rs` |
| Change password page | `src/changeui.rs` |
| AD password rules and password change (`NetUserChangePassword`) | `src/policy.rs` |
| Tray icon, balloon and menu | `src/tray.rs` |
| Windows helpers (time, registry, single instance) | `src/winutil.rs` |
| Settings file | `src/settings.rs` |
| Tray/window icon (drawn in code) | `src/icon.rs` |

- Days left come from the user's `msDS-UserPasswordExpiryTimeComputed` attribute (local accounts: password age vs.
  maximum age).
- Password rules come from the user's fine-grained policy (`msDS-ResultantPSO`) when readable, else the domain policy.
- Passwords typed in the app are only passed to Windows and wiped from memory afterwards; nothing is stored.
- If a server can't draw the window (no OpenGL 2), the tray icon, checks and warnings keep working.
