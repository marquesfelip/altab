//! Janela de Opções, no estilo das Preferências do macOS: grupos em cartões
//! arredondados, linhas com separador e marca de seleção azul.

use std::cell::{Cell, RefCell};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::autostart;
use crate::gfx::{Align, Target};
use crate::i18n::{self, Label, Lang};
use crate::render;
use crate::settings::{self, Mode, Theme};
use crate::windows_list::to_wide;

/// Enviada à janela principal quando uma opção muda.
pub const WM_SETTINGS_CHANGED: u32 = WM_APP + 11;

const CLIENT_W: f32 = 440.0;
const MARGIN: f32 = 20.0;
const TOP: f32 = 14.0;
const HEADER_H: f32 = 34.0;
const ROW_H: f32 = 46.0;
const GROUP_GAP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Mode(Mode),
    Theme(Theme),
    Lang(Lang),
    ToggleAutostart,
}

struct Item {
    text: &'static str,
    /// `None` = cabeçalho de grupo.
    action: Option<Action>,
    selected: bool,
    last_in_group: bool,
    rect: RECT,
}

struct Layout {
    items: Vec<Item>,
    cards: Vec<RECT>,
    /// Altura total do cliente, em pixels.
    height: i32,
}

struct State {
    main: HWND,
    scale: f32,
    focus: usize,
    target: Option<Target>,
}

thread_local! {
    static HWND_OPT: Cell<isize> = const { Cell::new(0) };
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn px(scale: f32, v: f32) -> i32 {
    (v * scale).round() as i32
}

fn layout(scale: f32) -> Layout {
    let st = settings::get();
    let mut items = Vec::new();
    let mut cards = Vec::new();
    let mut y = TOP;
    let left = px(scale, MARGIN);
    let right = px(scale, CLIENT_W - MARGIN);

    let mut group = |title: Label, rows: Vec<(Label, Action, bool)>| {
        items.push(Item {
            text: i18n::t(title),
            action: None,
            selected: false,
            last_in_group: false,
            rect: RECT {
                left: left + px(scale, 12.0),
                top: px(scale, y),
                right,
                bottom: px(scale, y + HEADER_H),
            },
        });
        y += HEADER_H;
        let card_top = px(scale, y);
        let n = rows.len();
        for (i, (label, action, selected)) in rows.into_iter().enumerate() {
            items.push(Item {
                text: i18n::t(label),
                action: Some(action),
                selected,
                last_in_group: i + 1 == n,
                rect: RECT { left, top: px(scale, y), right, bottom: px(scale, y + ROW_H) },
            });
            y += ROW_H;
        }
        cards.push(RECT { left, top: card_top, right, bottom: px(scale, y) });
        y += GROUP_GAP;
    };

    group(
        Label::ModeTitle,
        vec![
            (Label::ModeWindows, Action::Mode(Mode::Windows), st.mode == Mode::Windows),
            (Label::ModeFixed, Action::Mode(Mode::Fixed), st.mode == Mode::Fixed),
        ],
    );
    group(
        Label::ThemeTitle,
        vec![
            (Label::ThemeSystem, Action::Theme(Theme::System), st.theme == Theme::System),
            (Label::ThemeLight, Action::Theme(Theme::Light), st.theme == Theme::Light),
            (Label::ThemeDark, Action::Theme(Theme::Dark), st.theme == Theme::Dark),
        ],
    );
    group(
        Label::StartupTitle,
        vec![(Label::StartupLogin, Action::ToggleAutostart, st.autostart)],
    );
    group(
        Label::LanguageTitle,
        vec![
            (Label::LangPt, Action::Lang(Lang::PtBr), st.lang == Lang::PtBr),
            (Label::LangEn, Action::Lang(Lang::EnUs), st.lang == Lang::EnUs),
        ],
    );
    Layout { items, cards, height: px(scale, y + MARGIN - GROUP_GAP) }
}

/// Abre a janela de opções (ou traz a existente para a frente).
pub fn show(main: HWND) {
    unsafe {
        let existing = HWND(HWND_OPT.with(|h| h.get()) as *mut _);
        if !existing.0.is_null() && IsWindow(Some(existing)).as_bool() {
            if IsIconic(existing).as_bool() {
                let _ = ShowWindow(existing, SW_RESTORE);
            }
            let _ = SetForegroundWindow(existing);
            return;
        }

        let Ok(inst) = GetModuleHandleW(None) else { return };
        let class = w!("AltabOptions");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            hIcon: crate::tray::app_icon(GetSystemMetrics(SM_CXICON)),
            hIconSm: crate::tray::app_icon(GetSystemMetrics(SM_CXSMICON)),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let scale = GetDpiForSystem() as f32 / 96.0;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut r = RECT {
            left: 0,
            top: 0,
            right: px(scale, CLIENT_W),
            bottom: layout(scale).height,
        };
        let _ = AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE(0));
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let x = (GetSystemMetrics(SM_CXSCREEN) - w) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - h) / 2;

        STATE.with(|st| {
            *st.borrow_mut() = Some(State { main, scale, focus: 0, target: None });
        });
        let title = to_wide(i18n::t(Label::OptionsTitle));
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(title.as_ptr()),
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(inst.into()),
            None,
        ) else {
            return;
        };
        HWND_OPT.with(|c| c.set(hwnd.0 as isize));
        style_frame(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Barra de título no mesmo tom do fundo da janela, como nas Preferências do macOS.
fn style_frame(hwnd: HWND) {
    let dark = settings::effective_dark();
    render::apply_frame(hwnd, dark, None);
    let p = render::palette();
    let bgr = |c: crate::gfx::Rgba| c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16;
    render::apply_caption(hwnd, bgr(p.opt_bg), bgr(p.text));
}

fn apply(hwnd: HWND, action: Action) {
    settings::update(|st| match action {
        Action::Mode(m) => st.mode = m,
        Action::Theme(t) => st.theme = t,
        Action::Lang(l) => st.lang = l,
        Action::ToggleAutostart => st.autostart = !st.autostart,
    });
    if action == Action::ToggleAutostart {
        autostart::set(settings::get().autostart);
    }
    unsafe {
        let title = to_wide(i18n::t(Label::OptionsTitle));
        let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
        style_frame(hwnd);
        let _ = InvalidateRect(Some(hwnd), None, false);
        if let Some(main) = STATE.with(|st| st.borrow().as_ref().map(|s| s.main)) {
            let _ = PostMessageW(Some(main), WM_SETTINGS_CHANGED, WPARAM(0), LPARAM(0));
        }
    }
}

fn radio_indices(items: &[Item]) -> Vec<usize> {
    items.iter().enumerate().filter(|(_, i)| i.action.is_some()).map(|(n, _)| n).collect()
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let _ = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right.max(1) as u32, rc.bottom.max(1) as u32);

    STATE.with(|st| {
        let mut st = st.borrow_mut();
        let Some(st) = st.as_mut() else { return };
        if st.target.is_none() {
            st.target = Target::new(hwnd, w, h);
        }
        let (scale, focus, has_focus) = (st.scale, st.focus, GetFocus() == hwnd);
        let Some(t) = st.target.as_mut() else { return };
        t.resize(w, h);

        let p = render::palette();
        let lay = layout(scale);
        let radios = radio_indices(&lay.items);
        t.begin(p.opt_bg);

        for card in &lay.cards {
            t.rrect(*card, 10.0 * scale, p.opt_card);
        }
        for (n, it) in lay.items.iter().enumerate() {
            let r = it.rect;
            let Some(_) = it.action else {
                t.text(it.text, r, 12.5 * scale, 600, Align::Left, p.muted);
                continue;
            };
            if has_focus && radios.get(focus) == Some(&n) {
                let inset = px(scale, 4.0);
                let hr = RECT {
                    left: r.left + inset,
                    top: r.top + inset,
                    right: r.right - inset,
                    bottom: r.bottom - inset,
                };
                t.rrect(hr, 7.0 * scale, p.opt_hover);
            }
            let tr = RECT { left: r.left + px(scale, 16.0), right: r.right - px(scale, 44.0), ..r };
            t.text(it.text, tr, 14.0 * scale, 400, Align::Left, p.text);

            if it.selected {
                // Marca de seleção (✓) azul.
                let cx = r.right as f32 - 26.0 * scale;
                let cy = (r.top + r.bottom) as f32 / 2.0;
                let wd = 2.0 * scale;
                t.line(cx - 5.5 * scale, cy + 0.5 * scale, cx - 1.5 * scale, cy + 4.5 * scale, wd, p.accent);
                t.line(cx - 1.5 * scale, cy + 4.5 * scale, cx + 6.0 * scale, cy - 5.0 * scale, wd, p.accent);
            }
            if !it.last_in_group {
                let y = r.bottom as f32 - 0.5;
                t.line(
                    (r.left + px(scale, 16.0)) as f32,
                    y,
                    r.right as f32,
                    y,
                    1.0_f32.max(scale * 0.75),
                    p.line,
                );
            }
        }
        if !t.end() {
            st.target = None;
        }
    });
    let _ = EndPaint(hwnd, &ps);
}

fn activate_at(hwnd: HWND, list_index: usize, scale: f32) {
    let lay = layout(scale);
    if let Some(Some(a)) = lay.items.get(list_index).map(|i| i.action) {
        apply(hwnd, a);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
            let Some(scale) = STATE.with(|st| st.borrow().as_ref().map(|s| s.scale)) else {
                return LRESULT(0);
            };
            let lay = layout(scale);
            let radios = radio_indices(&lay.items);
            let hit = lay.items.iter().position(|i| {
                i.action.is_some()
                    && x >= i.rect.left
                    && x < i.rect.right
                    && y >= i.rect.top
                    && y < i.rect.bottom
            });
            if let Some(n) = hit {
                if let Some(f) = radios.iter().position(|&r| r == n) {
                    STATE.with(|st| {
                        if let Some(s) = st.borrow_mut().as_mut() {
                            s.focus = f;
                        }
                    });
                }
                let _ = SetFocus(Some(hwnd));
                activate_at(hwnd, n, scale);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = VIRTUAL_KEY(wp.0 as u16);
            let scale = STATE.with(|st| st.borrow().as_ref().map(|s| s.scale)).unwrap_or(1.0);
            let radios = radio_indices(&layout(scale).items);
            match vk {
                VK_ESCAPE => {
                    let _ = DestroyWindow(hwnd);
                }
                VK_DOWN | VK_UP => {
                    STATE.with(|st| {
                        if let Some(s) = st.borrow_mut().as_mut() {
                            let n = radios.len() as isize;
                            let d = if vk == VK_DOWN { 1 } else { -1 };
                            s.focus = (s.focus as isize + d).rem_euclid(n) as usize;
                        }
                    });
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                VK_RETURN | VK_SPACE => {
                    let f = STATE.with(|st| st.borrow().as_ref().map(|s| s.focus)).unwrap_or(0);
                    if let Some(&n) = radios.get(f) {
                        activate_at(hwnd, n, scale);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_DESTROY => {
            HWND_OPT.with(|c| c.set(0));
            STATE.with(|st| *st.borrow_mut() = None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
