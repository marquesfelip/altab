# Changelog

## 0.2.0

- New **Shortcut** option: choose the key combination that opens the switcher (default Alt+Tab).
  Click the row in Options and press the combination; it is validated (needs Alt or Ctrl, reserved
  keys are refused) and can be restored to Alt+Tab. Stored as `hotkey=` in `config.ini`.
- In *Windows default* mode, releasing the shortcut's modifier (Alt, Ctrl) selects the window.
- **Fix:** the C runtime is now linked statically, so altab starts on clean Windows installs without
  the Visual C++ Redistributable (previously it failed with `STATUS_DLL_NOT_FOUND`, 0xC0000135).
- Debug flag `--accept-injected` to drive the hook with synthetic key events in automated tests.

## 0.1.0

- First public release.
- Alt+Tab replacement with macOS-inspired UI (acrylic, Inter font, light/dark themes).
- Fuzzy search over app names and window titles, with highlighted matches.
- Live window previews (DWM thumbnails).
- Windows-default and Fixed behaviours; tray icon; Options window.
- English and Brazilian Portuguese UI; starts with Windows.
- Installer (Inno Setup), portable zip, `install.ps1`, winget manifest.
