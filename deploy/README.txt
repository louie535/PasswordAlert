Password Alert
==============

Warns users before their Windows / domain password expires. It sits in the tray (next to the clock):
- checks once a day, and every hour once 3 days or fewer are left;
- pops up a warning when 10 days or fewer are left (both numbers adjustable per user);
- a window with days left, the expiry date, Check now, and a Change password page that shows the
  domain's password rules and changes the password through Windows.

Requirements: 64-bit Windows 10 / Windows Server 2016 or newer. Nothing else to install
(no .NET, no runtime): PasswordAlert.exe is a single file.

Install on a server (all users)
-------------------------------
1. Copy this folder to the server.
2. Right-click Install.cmd > Run as administrator
   (or: powershell -ExecutionPolicy Bypass -File Install.ps1).
   - Copies PasswordAlert.exe to C:\Program Files\PasswordAlert
   - Starts it in the tray at every user's sign-in (HKLM Run key, "--tray")
   - Adds "Password Alert" to the Start menu
   Options: -InstallDir <folder>, -NoAutoStart, -NoShortcut
3. Users get it at their next sign-in. Running Install again upgrades in place.

Silent deployment (GPO, SCCM/Intune, PDQ...):
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File Install.ps1

Without installing: run PasswordAlert.exe from any folder; tick "Start with Windows" in its
window to start it at that user's sign-in.

Remove: run Uninstall.ps1 as administrator (also copied into the install folder).

Start options
-------------
  PasswordAlert.exe            start (minimized to the tray unless the user turned that off)
  PasswordAlert.exe --show     start with the window open
  PasswordAlert.exe --tray     start hidden in the tray (used at sign-in)
Starting it again while it runs opens the window of the running copy.

Per-user settings are kept in %APPDATA%\PasswordAlert\settings.json.

Notes
-----
- On a server whose graphics can't draw the window (no OpenGL 2), the tray icon, checks and
  warnings still work; the tray icon shows the message instead of opening the window.
- "Open Windows change password" opens Windows Security in remote-desktop sessions. At the
  console, Windows only allows Ctrl+Alt+Del, so the app's own change form is offered.
