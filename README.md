<p align="center">
  <img src="assets/icon/altab-512.png" width="128" alt="altab icon">
</p>

<h1 align="center">altab</h1>

<p align="center">
  A fast, good-looking <b>Alt+Tab replacement for Windows</b> with fuzzy search and live window previews.<br>
  <a href="README.pt-BR.md">Leia em português</a>
</p>

<p align="center">
  <a href="https://github.com/marquesfelip/altab/actions/workflows/ci.yml"><img src="https://github.com/marquesfelip/altab/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/marquesfelip/altab/releases/latest"><img src="https://img.shields.io/github/v/release/marquesfelip/altab" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

## Features

- **Alt+Tab, redesigned** – macOS-inspired panel with acrylic background, Inter font, light/dark themes.
- **Fuzzy search** – type `chr` for *Google Chrome*, `vsc` for *Visual Studio Code*, `opcoes` for *Opções*
  (case- and accent-insensitive; matches app names and window titles; matched letters are highlighted).
- **Live previews** of the selected window (DWM thumbnails).
- **Custom shortcut** – keep Alt+Tab or record your own combination (e.g. `Ctrl+Space`) in Options.
- **Two behaviours** – *Windows default* (release Alt to switch) or *Fixed* (stays open until Enter/Esc/click).
- **Tray icon** with Options and Exit, English and Brazilian Portuguese UI (follows your Windows language).
- **Starts with Windows**, no admin rights needed. Small (~1.5 MB), written in Rust, no telemetry.

## Install

Pick one. All of them install per user (no administrator prompt) and make altab start with Windows.

**winget** (available as soon as the [package request](https://github.com/microsoft/winget-pkgs/pull/450379) is approved)

```powershell
winget install marquesfelip.altab
```

**PowerShell one-liner** (downloads the latest release and verifies its SHA-256 checksum)

```powershell
irm https://raw.githubusercontent.com/marquesfelip/altab/main/install.ps1 | iex
```

**Manual** – download `altab-setup.exe` (installer) or `altab-portable.zip` from the
[latest release](https://github.com/marquesfelip/altab/releases/latest).

> Windows SmartScreen may warn about an unrecognized app until the binary builds reputation
> (see [Code signing policy](#code-signing-policy)). Choose *More info → Run anyway*.
> Every release ships a `SHA256SUMS` file and a [build provenance attestation](https://docs.github.com/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds)
> you can verify with `gh attestation verify altab-setup.exe --repo marquesfelip/altab`.

### Update

`winget upgrade marquesfelip.altab`, or run the one-liner again.

### Uninstall

```powershell
winget uninstall marquesfelip.altab
```

or use *Settings → Apps*, or:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/marquesfelip/altab/main/install.ps1))) -Uninstall
```

## Usage

| Key | Action |
| --- | --- |
| `Alt` + `Tab` (configurable) | Open the switcher / next app |
| `Shift` + `Alt` + `Tab`, `↑` `←` | Previous app |
| `Tab`, `↓` `→`, mouse wheel | Next app |
| *type text* | Fuzzy search (keeps the panel open) |
| `Enter` / click | Switch to the selected window |
| `Esc` / click outside | Cancel |

Click the tray icon for **Options**: Alt+Tab behaviour, shortcut, theme, start with Windows and language.
To change the shortcut, click the *Switcher shortcut* row and press the new combination (it needs `Alt` or `Ctrl`).
Settings are stored in `%APPDATA%\altab\config.ini`:

```ini
mode=windows     # windows | fixed
theme=system     # system | light | dark
language=en-US   # en-US | pt-BR
autostart=true   # true | false
hotkey=alt+tab   # e.g. ctrl+space, alt+grave, ctrl+alt+k
```

**Known limitation:** when a window running as administrator is focused, Windows does not deliver
keystrokes to the hook and the native Alt+Tab appears. Run `altab.exe` as administrator to cover that case.

## Privacy

altab runs entirely on your machine. It does not use the network and collects no data. It reads the list of
open windows (titles, icons, thumbnails) only to show the switcher, and keeps your settings in `config.ini`.

## Code signing policy

Release binaries are built from this repository by [GitHub Actions](.github/workflows/release.yml)
(no manual steps) and carry a signed [build provenance attestation](https://docs.github.com/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds).

Authenticode signing is provided free of charge by the [SignPath Foundation](https://signpath.org/)
for open source projects, once the project is onboarded. Roles:

- **Author / committer / reviewer / approver:** [@marquesfelip](https://github.com/marquesfelip)

Privacy: see [Privacy](#privacy). Until the first signed release, binaries are unsigned and SmartScreen
may show a warning.

## Build from source

Requires Windows, Rust (stable, MSVC toolchain) and the Windows SDK.

```powershell
cargo build --release     # target\release\altab.exe
cargo test
```

The installer is built with [Inno Setup 6](https://jrsoftware.org/isinfo.php):

```powershell
ISCC.exe /DAppVersion=0.1.0 installer\altab.iss   # dist\altab-setup.exe
```

Debug flags: `altab.exe --show` (open the switcher), `--options` (open Options), `--query <text>` (prefill the search), `--accept-injected` (process synthetic key events, for automated tests).

## Releasing (maintainers)

1. Bump `version` in `Cargo.toml` and update `CHANGELOG.md`.
2. `git tag vX.Y.Z && git push --tags` – the *Release* workflow builds, tests, packages, attests and publishes.
3. With the `WINGET_TOKEN` secret configured, the workflow opens the winget-pkgs update PR automatically.
4. Optional signing: set the `SIGNPATH_ORGANIZATION_ID` repository variable and the `SIGNPATH_API_TOKEN` secret.

## Credits and licenses

- altab is released under the [MIT License](LICENSE).
- [Inter](https://rsms.me/inter/) font – SIL Open Font License 1.1 ([`assets/fonts/Inter-LICENSE.txt`](assets/fonts/Inter-LICENSE.txt)).
- [`nucleo-matcher`](https://github.com/helix-editor/nucleo) – MPL-2.0 (fuzzy matching).
- [`windows`](https://github.com/microsoft/windows-rs) crate – MIT / Apache-2.0.
