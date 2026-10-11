//! Atalho que abre o seletor: modificadores (Alt/Ctrl/Shift) + uma tecla.
//! Lógica pura (serialização, validação, nomes) para ser testável.

use crate::i18n::Lang;

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_SPACE: u16 = 0x20;
pub const VK_DELETE: u16 = 0x2E;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub vk: u16,
}

/// Alt+Tab, o padrão.
pub const DEFAULT: Hotkey = Hotkey { alt: true, ctrl: false, shift: false, vk: VK_TAB };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invalid {
    /// Sem Alt nem Ctrl, o atalho atrapalharia a digitação normal.
    NeedsModifier,
    /// Tecla com função própria no seletor, ou reservada pelo sistema.
    Reserved,
}

/// Nomes estáveis (independentes de idioma) usados no arquivo de configuração.
const NAMED: &[(&str, u16)] = &[
    ("tab", VK_TAB),
    ("space", VK_SPACE),
    ("grave", 0xC0),
    ("minus", 0xBD),
    ("equals", 0xBB),
    ("comma", 0xBC),
    ("period", 0xBE),
    ("slash", 0xBF),
    ("backslash", 0xDC),
    ("semicolon", 0xBA),
    ("quote", 0xDE),
    ("lbracket", 0xDB),
    ("rbracket", 0xDD),
    ("insert", 0x2D),
    ("home", 0x24),
    ("end", 0x23),
    ("pageup", 0x21),
    ("pagedown", 0x22),
];

impl Hotkey {
    pub const fn pack(self) -> u32 {
        (self.alt as u32) | (self.ctrl as u32) << 1 | (self.shift as u32) << 2 | (self.vk as u32) << 8
    }

    pub const fn unpack(v: u32) -> Self {
        Hotkey { alt: v & 1 != 0, ctrl: v & 2 != 0, shift: v & 4 != 0, vk: (v >> 8) as u16 }
    }

    /// Texto gravado no `config.ini`, ex.: `ctrl+alt+tab`.
    pub fn to_config(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("ctrl+");
        }
        if self.alt {
            s.push_str("alt+");
        }
        if self.shift {
            s.push_str("shift+");
        }
        s.push_str(&key_config_name(self.vk));
        s
    }

    pub fn parse(text: &str) -> Option<Hotkey> {
        let mut hk = Hotkey { alt: false, ctrl: false, shift: false, vk: 0 };
        for part in text.split('+').map(|p| p.trim().to_lowercase()) {
            match part.as_str() {
                "" => return None,
                "alt" => hk.alt = true,
                "ctrl" | "control" => hk.ctrl = true,
                "shift" => hk.shift = true,
                key => {
                    if hk.vk != 0 {
                        return None;
                    }
                    hk.vk = parse_key(key)?;
                }
            }
        }
        (hk.vk != 0).then_some(hk)
    }

    pub fn validate(self) -> Result<(), Invalid> {
        if !(self.alt || self.ctrl) {
            return Err(Invalid::NeedsModifier);
        }
        let vk = self.vk;
        // Teclas de modificadores nunca são a tecla principal.
        if matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
            return Err(Invalid::NeedsModifier);
        }
        // Teclas com função dentro do seletor aberto, e o Delete (Ctrl+Alt+Del).
        if matches!(vk, VK_BACK | VK_RETURN | VK_ESCAPE | VK_DELETE | 0x25..=0x28) {
            return Err(Invalid::Reserved);
        }
        // Alt+F4 fecha janelas.
        if self.alt && vk == 0x73 {
            return Err(Invalid::Reserved);
        }
        // Atalhos de edição universais: Ctrl + A/C/V/X/Y/Z/S (sem Alt).
        if self.ctrl && !self.alt && matches!(vk, 0x41 | 0x43 | 0x56 | 0x58 | 0x59 | 0x5A | 0x53) {
            return Err(Invalid::Reserved);
        }
        Ok(())
    }

    /// Texto para exibição, ex.: `Ctrl + Alt + Tab`.
    pub fn display(self, lang: Lang) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(key_display_name(self.vk, lang));
        parts.join(" + ")
    }
}

fn key_config_name(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_ascii_lowercase().to_string(),
        0x70..=0x87 => format!("f{}", vk - 0x6F),
        _ => match NAMED.iter().find(|(_, v)| *v == vk) {
            Some((n, _)) => (*n).to_string(),
            None => format!("vk{vk:#04x}"),
        },
    }
}

fn parse_key(name: &str) -> Option<u16> {
    if let Some(&(_, v)) = NAMED.iter().find(|(n, _)| *n == name) {
        return Some(v);
    }
    if name.len() == 1 {
        let c = name.as_bytes()[0].to_ascii_uppercase();
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    let hex = name.strip_prefix("vk0x")?;
    u16::from_str_radix(hex, 16).ok().filter(|v| (1..=0xFE).contains(v))
}

fn key_display_name(vk: u16, lang: Lang) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        VK_TAB => "Tab".into(),
        VK_SPACE => match lang {
            Lang::PtBr => "Espaço".into(),
            Lang::EnUs => "Space".into(),
        },
        0xC0 => "`".into(),
        0xBD => "-".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xDC => "\\".into(),
        0xBA => ";".into(),
        0xDE => "'".into(),
        0xDB => "[".into(),
        0xDD => "]".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "Page Up".into(),
        0x22 => "Page Down".into(),
        _ => format!("0x{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(s: &str) -> Hotkey {
        Hotkey::parse(s).unwrap()
    }

    #[test]
    fn default_is_alt_tab() {
        assert_eq!(DEFAULT.to_config(), "alt+tab");
        assert_eq!(hk("alt+tab"), DEFAULT);
        assert!(DEFAULT.validate().is_ok());
    }

    #[test]
    fn config_roundtrip() {
        for s in ["alt+tab", "ctrl+space", "ctrl+alt+k", "alt+grave", "ctrl+shift+f9", "alt+1"] {
            assert_eq!(hk(s).to_config(), s);
        }
    }

    #[test]
    fn pack_roundtrip() {
        let h = hk("ctrl+alt+shift+f12");
        assert_eq!(Hotkey::unpack(h.pack()), h);
    }

    #[test]
    fn parse_is_tolerant_and_strict() {
        assert_eq!(hk(" Ctrl + Space "), hk("ctrl+space"));
        assert_eq!(hk("ALT+TAB"), DEFAULT);
        assert!(Hotkey::parse("").is_none());
        assert!(Hotkey::parse("alt+").is_none());
        assert!(Hotkey::parse("alt+tab+space").is_none());
        assert!(Hotkey::parse("alt+banana").is_none());
        assert!(Hotkey::parse("alt").is_none());
    }

    #[test]
    fn validation_rules() {
        assert_eq!(hk("shift+a").validate(), Err(Invalid::NeedsModifier));
        assert_eq!(hk("a").validate(), Err(Invalid::NeedsModifier));
        assert!(hk("ctrl+space").validate().is_ok());
        assert!(hk("alt+grave").validate().is_ok());
        assert!(hk("ctrl+alt+tab").validate().is_ok());
        assert_eq!(hk("alt+f4").validate(), Err(Invalid::Reserved));
        assert_eq!(hk("ctrl+c").validate(), Err(Invalid::Reserved));
        assert!(hk("ctrl+alt+c").validate().is_ok());
        assert_eq!(hk("alt+vk0x25").validate(), Err(Invalid::Reserved));
        assert_eq!(hk("ctrl+alt+vk0x2e").validate(), Err(Invalid::Reserved));
    }

    #[test]
    fn display_names() {
        assert_eq!(DEFAULT.display(Lang::EnUs), "Alt + Tab");
        assert_eq!(hk("ctrl+space").display(Lang::PtBr), "Ctrl + Espaço");
        assert_eq!(hk("ctrl+alt+f5").display(Lang::EnUs), "Ctrl + Alt + F5");
    }
}
