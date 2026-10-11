//! Visual do seletor (estilo macOS): fundo acrílico, campo de pesquisa em pílula,
//! seleção azul arredondada e card de preview. O preview em si é composto pelo
//! DWM por cima da área reservada em `Layout::thumb_rect`.

use std::ffi::c_void;

use windows::core::s;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::gfx::{Align, Rgba, Target};
use crate::i18n::{self, Label};
use crate::settings;
use crate::switcher::App;

// Medidas em DIP (96 dpi).
const W: f32 = 1000.0;
const H: f32 = 610.0;
const LIST_X: f32 = 12.0;
const LIST_W: f32 = 288.0;
const LIST_TOP: f32 = 82.0;
const ITEM_H: f32 = 52.0;
const ITEM_PITCH: f32 = 58.0;
const BODY_BOTTOM: f32 = H - 52.0;
const CARD_X: f32 = 316.0;
const CARD_HEADER: f32 = 44.0;

pub struct Palette {
    /// Fundo sólido usado quando o acrílico não está disponível.
    pub bg_solid: Rgba,
    /// Véu sobre o acrílico, para garantir contraste do texto.
    pub tint: Rgba,
    pub field: Rgba,
    pub accent: Rgba,
    pub sel_text: Rgba,
    pub card: Rgba,
    pub thumb_bg: Rgba,
    pub line: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    /// Janela de opções (sólida, sem acrílico).
    pub opt_bg: Rgba,
    pub opt_card: Rgba,
    pub opt_hover: Rgba,
}

const DARK: Palette = Palette {
    bg_solid: (34, 34, 36, 255),
    tint: (24, 24, 26, 110),
    field: (255, 255, 255, 24),
    accent: (10, 132, 255, 255),
    sel_text: (255, 255, 255, 255),
    card: (255, 255, 255, 12),
    thumb_bg: (0, 0, 0, 70),
    line: (255, 255, 255, 28),
    text: (255, 255, 255, 235),
    muted: (255, 255, 255, 140),
    opt_bg: (28, 28, 30, 255),
    opt_card: (44, 44, 46, 255),
    opt_hover: (58, 58, 60, 255),
};

const LIGHT: Palette = Palette {
    bg_solid: (244, 244, 246, 255),
    tint: (250, 250, 252, 120),
    field: (0, 0, 0, 14),
    accent: (0, 122, 255, 255),
    sel_text: (255, 255, 255, 255),
    card: (255, 255, 255, 150),
    thumb_bg: (0, 0, 0, 16),
    line: (0, 0, 0, 26),
    text: (0, 0, 0, 225),
    muted: (0, 0, 0, 120),
    opt_bg: (242, 242, 247, 255),
    opt_card: (255, 255, 255, 255),
    opt_hover: (234, 234, 240, 255),
};

pub fn palette() -> &'static Palette {
    if settings::effective_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

/// Aplica o modo escuro da moldura (e, opcionalmente, a cor da borda) via DWM.
pub fn apply_frame(hwnd: HWND, dark: bool, border: Option<u32>) {
    unsafe {
        let v = dark as i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &v as *const _ as *const c_void,
            4,
        );
        if let Some(c) = border {
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &c as *const _ as *const c_void, 4);
        }
    }
}

/// Cores da barra de título (COLORREF 0x00BBGGRR) para a janela de Opções.
pub fn apply_caption(hwnd: HWND, caption: u32, text: u32) {
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as *const c_void, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const _ as *const c_void, 4);
    }
}

/// Liga o fundo acrílico atrás da janela. Retorna `false` se indisponível;
/// o chamador então pinta um fundo sólido.
pub fn apply_backdrop(hwnd: HWND, dark: bool) -> bool {
    // Caminho documentado (Windows 11 22H2+): backdrop "transient window" = acrílico.
    unsafe {
        let kind: i32 = 3; // DWMSBT_TRANSIENTWINDOW
        let ok = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &kind as *const _ as *const c_void,
            4,
        )
        .is_ok();
        if ok {
            let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
            if DwmExtendFrameIntoClientArea(hwnd, &m).is_ok() {
                return true;
            }
        }
    }
    apply_backdrop_legacy(hwnd, dark)
}

/// Fallback: política de "accent" não documentada do user32.
fn apply_backdrop_legacy(hwnd: HWND, dark: bool) -> bool {
    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        gradient: u32,
        animation: u32,
    }
    #[repr(C)]
    struct CompositionData {
        attribute: u32,
        data: *mut c_void,
        size: usize,
    }
    type SetWca = unsafe extern "system" fn(HWND, *mut CompositionData) -> i32;

    unsafe {
        let Ok(user32) = GetModuleHandleW(windows::core::w!("user32.dll")) else {
            return false;
        };
        let Some(proc) = GetProcAddress(user32, s!("SetWindowCompositionAttribute")) else {
            return false;
        };
        let set: SetWca = std::mem::transmute(proc);
        // Cor do tint no formato AABBGGRR.
        let tint: u32 = if dark { 0x99_24_22_22 } else { 0xA6_F6_F4_F4 };
        let mut policy = AccentPolicy { state: 4, flags: 2, gradient: tint, animation: 0 };
        let mut data = CompositionData {
            attribute: 19,
            data: &mut policy as *mut _ as *mut c_void,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        set(hwnd, &mut data) != 0
    }
}

#[derive(Clone, Copy)]
pub struct Layout {
    pub scale: f32,
}

impl Layout {
    pub fn new(scale: f32) -> Self {
        Self { scale }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    pub fn width(&self) -> i32 {
        self.s(W)
    }

    pub fn height(&self) -> i32 {
        self.s(H)
    }

    pub fn visible_items(&self) -> usize {
        ((BODY_BOTTOM - LIST_TOP) / ITEM_PITCH).floor() as usize
    }

    fn rect(&self, l: f32, t: f32, r: f32, b: f32) -> RECT {
        RECT { left: self.s(l), top: self.s(t), right: self.s(r), bottom: self.s(b) }
    }

    fn item_rect(&self, row: usize) -> RECT {
        let top = LIST_TOP + row as f32 * ITEM_PITCH;
        self.rect(LIST_X, top, LIST_X + LIST_W, top + ITEM_H)
    }

    fn card_rect(&self) -> RECT {
        self.rect(CARD_X, LIST_TOP, W - 16.0, BODY_BOTTOM)
    }

    /// Área (em pixels do cliente) onde a miniatura do DWM é desenhada.
    pub fn thumb_rect(&self) -> RECT {
        self.rect(CARD_X + 14.0, LIST_TOP + CARD_HEADER, W - 30.0, BODY_BOTTOM - 14.0)
    }

    /// Índice (na lista filtrada) do item sob o ponto, considerando a rolagem.
    pub fn item_at(&self, x: i32, y: i32, scroll: usize) -> Option<usize> {
        for row in 0..self.visible_items() {
            let r = self.item_rect(row);
            if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                return Some(scroll + row);
            }
        }
        None
    }
}

pub fn paint(app: &mut App) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let _ = BeginPaint(app.hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(app.hwnd, &mut rc);
        let (w, h) = (rc.right.max(1) as u32, rc.bottom.max(1) as u32);

        let mut target = app.target.take().or_else(|| Target::new(app.hwnd, w, h));
        if let Some(t) = target.as_mut() {
            t.resize(w, h);
            draw(t, app);
            if !t.end() {
                target = None; // dispositivo perdido: recria no próximo paint
            }
        }
        app.target = target;
        let _ = EndPaint(app.hwnd, &ps);
    }
}

fn draw(t: &Target, app: &App) {
    let l = app.layout;
    let sc = l.scale;
    let sz = |v: f32| l.s(v);
    let p = palette();
    t.begin(if app.acrylic { p.tint } else { p.bg_solid });

    // Campo de pesquisa em pílula.
    let field = l.rect(16.0, 16.0, W - 16.0, 60.0);
    t.rrect(field, 12.0 * sc, p.field);
    let (mx, my) = (field.left as f32 + 28.0 * sc, field.top as f32 + 22.0 * sc);
    t.circle_stroke(mx, my, 6.5 * sc, 1.8 * sc, p.muted);
    t.line(mx + 4.8 * sc, my + 4.8 * sc, mx + 9.5 * sc, my + 9.5 * sc, 1.8 * sc, p.muted);
    let trect = RECT { left: field.left + sz(52.0), right: field.right - sz(14.0), ..field };
    if app.query.is_empty() {
        t.text(i18n::t(Label::SearchPlaceholder), trect, 17.0 * sc, 400, Align::Left, p.muted);
    } else {
        t.text(&app.query, trect, 17.0 * sc, 400, Align::Left, p.text);
    }

    // Lista de aplicativos.
    let icon = sz(32.0);
    for row in 0..l.visible_items() {
        let idx = app.scroll + row;
        let Some(m) = app.filtered.get(idx) else { break };
        let entry = &app.entries[m.app];
        let r = l.item_rect(row);
        let selected = idx == app.sel;
        if selected {
            t.rrect(r, 10.0 * sc, p.accent);
        }
        let iy = r.top + (r.bottom - r.top - icon) / 2;
        t.icon(
            entry.icon,
            RECT { left: r.left + sz(12.0), top: iy, right: r.left + sz(12.0) + icon, bottom: iy + icon },
        );
        let tr = RECT { left: r.left + sz(56.0), right: r.right - sz(12.0), ..r };
        let (color, weight) = if selected { (p.sel_text, 500) } else { (p.text, 400) };
        // Letras casadas: azul no item comum; sublinhadas e brancas no selecionado.
        let hit_color = if selected { p.sel_text } else { p.accent };
        t.text_marked(&entry.name, tr, 15.0 * sc, weight, color, hit_color, selected, &m.name_hits);
    }

    // Card de preview.
    let card = l.card_rect();
    t.rrect(card, 14.0 * sc, p.card);
    if let Some(entry) = app.selected() {
        let small = sz(20.0);
        let hy = card.top + (sz(CARD_HEADER) - small) / 2;
        t.icon(
            entry.icon,
            RECT { left: card.left + sz(16.0), top: hy, right: card.left + sz(16.0) + small, bottom: hy + small },
        );
        let title = app.selected_window().map(|w| w.title.as_str()).unwrap_or("");
        let hr = RECT {
            left: card.left + sz(46.0),
            top: card.top,
            right: card.right - sz(16.0),
            bottom: card.top + sz(CARD_HEADER),
        };
        t.text(title, hr, 13.0 * sc, 600, Align::Left, p.text);
        t.rrect(l.thumb_rect(), 8.0 * sc, p.thumb_bg);
    }

    // Rodapé.
    let n = app.window_count();
    let left = format!(
        "{} {}",
        n,
        i18n::t(if n == 1 { Label::WindowOpenOne } else { Label::WindowsOpenMany })
    );
    let fr = l.rect(28.0, H - 48.0, W - 28.0, H - 8.0);
    t.text(&left, fr, 12.0 * sc, 400, Align::Left, p.muted);
    t.text(i18n::t(Label::PressTab), fr, 12.0 * sc, 400, Align::Right, p.muted);
}
