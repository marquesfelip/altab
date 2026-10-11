//! Configurações do usuário, persistidas em `%APPDATA%\altab\config.ini`.

use std::cell::Cell;
use std::ffi::c_void;
use std::path::PathBuf;

use windows::core::w;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

use crate::hotkey::{self, Hotkey};
use crate::i18n::{self, Lang};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Soltar o Alt ativa o app selecionado.
    Windows,
    /// O seletor permanece aberto após soltar o Alt.
    Fixed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub mode: Mode,
    pub theme: Theme,
    pub lang: Lang,
    /// Iniciar junto com o Windows (padrão: ligado).
    pub autostart: bool,
    /// Atalho que abre o seletor (padrão: Alt+Tab).
    pub hotkey: Hotkey,
}

thread_local! {
    static CURRENT: Cell<Option<Settings>> = const { Cell::new(None) };
}

pub fn get() -> Settings {
    CURRENT.with(|c| {
        c.get().unwrap_or_else(|| {
            let s = load();
            c.set(Some(s));
            s
        })
    })
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    let mut s = get();
    f(&mut s);
    CURRENT.with(|c| c.set(Some(s)));
    save(&s);
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("altab").join("config.ini"))
}

fn load() -> Settings {
    let mut s = Settings {
        mode: Mode::Windows,
        theme: Theme::System,
        lang: i18n::detect_system(),
        autostart: true,
        hotkey: hotkey::DEFAULT,
    };
    let Some(text) = config_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return s;
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match (k.trim(), v.trim()) {
            ("mode", "windows") => s.mode = Mode::Windows,
            ("mode", "fixed") => s.mode = Mode::Fixed,
            ("theme", "system") => s.theme = Theme::System,
            ("theme", "light") => s.theme = Theme::Light,
            ("theme", "dark") => s.theme = Theme::Dark,
            ("hotkey", text) => {
                if let Some(h) = Hotkey::parse(text).filter(|h| h.validate().is_ok()) {
                    s.hotkey = h;
                }
            }
            ("autostart", "true") => s.autostart = true,
            ("autostart", "false") => s.autostart = false,
            ("language", code) => {
                if let Some(l) = Lang::from_code(code) {
                    s.lang = l;
                }
            }
            _ => {}
        }
    }
    s
}

fn save(s: &Settings) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body = format!(
        "mode={}\ntheme={}\nlanguage={}\nautostart={}\nhotkey={}\n",
        match s.mode {
            Mode::Windows => "windows",
            Mode::Fixed => "fixed",
        },
        match s.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        },
        s.lang.code(),
        s.autostart,
        s.hotkey.to_config()
    );
    let _ = std::fs::write(path, body);
}

/// Tema escuro do app segundo a preferência do Windows (padrão: escuro).
fn system_prefers_dark() -> bool {
    let mut data = 1u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    r != ERROR_SUCCESS || data == 0
}

pub fn effective_dark() -> bool {
    match get().theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => system_prefers_dark(),
    }
}
