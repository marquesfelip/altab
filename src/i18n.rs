//! Labels localizados (en-US / pt-BR). O idioma efetivo vem de `settings`.

use windows::Win32::Globalization::GetUserDefaultLocaleName;

use crate::settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    EnUs,
    PtBr,
}

#[derive(Clone, Copy)]
pub enum Label {
    SearchPlaceholder,
    WindowOpenOne,
    WindowsOpenMany,
    PressTab,
    TrayTooltip,
    TrayOptions,
    TrayExit,
    OptionsTitle,
    ModeTitle,
    ModeWindows,
    ModeFixed,
    ThemeTitle,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    StartupTitle,
    StartupLogin,
    LanguageTitle,
    LangPt,
    LangEn,
}

impl Lang {
    /// Código gravado no arquivo de configuração.
    pub fn code(self) -> &'static str {
        match self {
            Lang::EnUs => "en-US",
            Lang::PtBr => "pt-BR",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "en-US" => Some(Lang::EnUs),
            "pt-BR" => Some(Lang::PtBr),
            _ => None,
        }
    }
}

/// Idioma do Windows: qualquer `pt-*` vira pt-BR; o resto, en-US.
pub fn detect_system() -> Lang {
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    let name = String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize]).to_lowercase();
    if name == "pt" || name.starts_with("pt-") {
        Lang::PtBr
    } else {
        Lang::EnUs
    }
}

pub fn t(label: Label) -> &'static str {
    text(settings::get().lang, label)
}

pub fn text(lang: Lang, label: Label) -> &'static str {
    use Label::*;
    match lang {
        Lang::EnUs => match label {
            SearchPlaceholder => "Search open applications",
            WindowOpenOne => "window open",
            WindowsOpenMany => "windows open",
            PressTab => "Press Tab to switch",
            TrayTooltip => "altab - Alt+Tab switcher",
            TrayOptions => "Options",
            TrayExit => "Exit",
            OptionsTitle => "altab Options",
            ModeTitle => "Alt+Tab behavior",
            ModeWindows => "Windows default (closes when Alt is released)",
            ModeFixed => "Fixed (stays open after Alt is released)",
            ThemeTitle => "Theme",
            ThemeSystem => "System default",
            ThemeLight => "Light",
            ThemeDark => "Dark",
            StartupTitle => "Startup",
            StartupLogin => "Start with Windows",
            LanguageTitle => "Language",
            LangPt => "Português (Brasil)",
            LangEn => "English (International)",
        },
        Lang::PtBr => match label {
            SearchPlaceholder => "Pesquisar aplicativos abertos",
            WindowOpenOne => "janela aberta",
            WindowsOpenMany => "janelas abertas",
            PressTab => "Pressione Tab para alternar",
            TrayTooltip => "altab - seletor de janelas",
            TrayOptions => "Opções",
            TrayExit => "Sair",
            OptionsTitle => "Opções do altab",
            ModeTitle => "Comportamento do Alt+Tab",
            ModeWindows => "Padrão Windows (fecha ao soltar o Alt)",
            ModeFixed => "Fixo (permanece aberto após soltar o Alt)",
            ThemeTitle => "Tema",
            ThemeSystem => "Padrão do sistema",
            ThemeLight => "Claro",
            ThemeDark => "Escuro",
            StartupTitle => "Inicialização",
            StartupLogin => "Iniciar com o Windows",
            LanguageTitle => "Idioma",
            LangPt => "Português (Brasil)",
            LangEn => "English (International)",
        },
    }
}
