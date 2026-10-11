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
use crate::gfx::{Align, Rgba, Target};
use crate::hook;
use crate::hotkey::{self, Hotkey, Invalid};
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
const NOTE_H: f32 = 30.0;
const GROUP_GAP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Mode(Mode),
    Theme(Theme),
    Lang(Lang),
    ToggleAutostart,
    RecordHotkey,
    ResetHotkey,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Header,
    Row,
    Note,
}

struct Item {
    text: String,
    kind: Kind,
    action: Option<Action>,
    /// Mostra a marca de seleção (✓).
    selected: bool,
    /// Texto à direita da linha (ex.: o atalho atual).
    value: Option<String>,
    value_accent: bool,
    last_in_group: bool,
    rect: RECT,
}

struct Layout {
    items: Vec<Item>,
    cards: Vec<RECT>,
    /// Altura total do cliente, em pixels.
    height: i32,
}

/// Estado transitório da interface.
#[derive(Clone, Copy, Default)]
struct Ui {
    recording: bool,
    error: Option<Label>,
}

struct State {
    main: HWND,
    scale: f32,
    focus: usize,
    ui: Ui,
    target: Option<Target>,
}

thread_local! {
    static HWND_OPT: Cell<isize> = const { Cell::new(0) };
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn px(scale: f32, v: f32) -> i32 {
    (v * scale).round() as i32
}

struct Row {
    text: String,
    action: Action,
    selected: bool,
    value: Option<(String, bool)>,
}

fn row(label: Label, action: Action, selected: bool) -> Row {
    Row { text: i18n::t(label).to_string(), action, selected, value: None }
}

fn layout(scale: f32, ui: Ui) -> Layout {
    let st = settings::get();
    let mut items = Vec::new();
    let mut cards = Vec::new();
    let mut y = TOP;
    let left = px(scale, MARGIN);
    let right = px(scale, CLIENT_W - MARGIN);

    let mut group = |title: Label, rows: Vec<Row>, note: Option<Label>| {
        items.push(Item {
            text: i18n::t(title).to_string(),
            kind: Kind::Header,
            action: None,
            selected: false,
            value: None,
            value_accent: false,
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
        for (i, r) in rows.into_iter().enumerate() {
            let (value, value_accent) = match r.value {
                Some((v, a)) => (Some(v), a),
                None => (None, false),
            };
            items.push(Item {
                text: r.text,
                kind: Kind::Row,
                action: Some(r.action),
                selected: r.selected,
                value,
                value_accent,
                last_in_group: i + 1 == n,
                rect: RECT { left, top: px(scale, y), right, bottom: px(scale, y + ROW_H) },
            });
            y += ROW_H;
        }
        cards.push(RECT { left, top: card_top, right, bottom: px(scale, y) });
        if let Some(label) = note {
            items.push(Item {
                text: i18n::t(label).to_string(),
                kind: Kind::Note,
                action: None,
                selected: false,
                value: None,
                value_accent: false,
                last_in_group: false,
                rect: RECT {
                    left: left + px(scale, 12.0),
                    top: px(scale, y),
                    right,
                    bottom: px(scale, y + NOTE_H),
                },
            });
            y += NOTE_H;
        }
        y += GROUP_GAP;
    };

    group(
        Label::ModeTitle,
        vec![
            row(Label::ModeWindows, Action::Mode(Mode::Windows), st.mode == Mode::Windows),
            row(Label::ModeFixed, Action::Mode(Mode::Fixed), st.mode == Mode::Fixed),
        ],
        None,
    );

    let hk_value = if ui.recording {
        (i18n::t(Label::HotkeyRecording).to_string(), true)
    } else {
        (st.hotkey.display(st.lang), false)
    };
    let mut hk_row = row(Label::HotkeyRow, Action::RecordHotkey, false);
    hk_row.value = Some(hk_value);
    let note = ui.error;
    group(
        Label::HotkeyTitle,
        vec![hk_row, row(Label::HotkeyReset, Action::ResetHotkey, false)],
        note,
    );

    group(
        Label::ThemeTitle,
        vec![
            row(Label::ThemeSystem, Action::Theme(Theme::System), st.theme == Theme::System),
            row(Label::ThemeLight, Action::Theme(Theme::Light), st.theme == Theme::Light),
            row(Label::ThemeDark, Action::Theme(Theme::Dark), st.theme == Theme::Dark),
        ],
        None,
    );
    group(
        Label::StartupTitle,
        vec![row(Label::StartupLogin, Action::ToggleAutostart, st.autostart)],
        None,
    );
    group(
        Label::LanguageTitle,
        vec![
            row(Label::LangPt, Action::Lang(Lang::PtBr), st.lang == Lang::PtBr),
            row(Label::LangEn, Action::Lang(Lang::EnUs), st.lang == Lang::EnUs),
        ],
        None,
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
        // Reserva a altura do caso mais alto (com a nota de erro visível).
        let tall = layout(scale, Ui { recording: false, error: Some(Label::HotkeyReserved) });
        let mut r = RECT { left: 0, top: 0, right: px(scale, CLIENT_W), bottom: tall.height };
        let _ = AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE(0));
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let x = (GetSystemMetrics(SM_CXSCREEN) - w) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - h) / 2;

        STATE.with(|st| {
            *st.borrow_mut() =
                Some(State { main, scale, focus: 0, ui: Ui::default(), target: None });
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
    let bgr = |c: Rgba| c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16;
    render::apply_caption(hwnd, bgr(p.opt_bg), bgr(p.text));
}

fn notify_main() {
    if let Some(main) = STATE.with(|st| st.borrow().as_ref().map(|s| s.main)) {
        unsafe {
            let _ = PostMessageW(Some(main), WM_SETTINGS_CHANGED, WPARAM(0), LPARAM(0));
        }
    }
}

fn set_ui(f: impl FnOnce(&mut Ui)) {
    STATE.with(|st| {
        if let Some(s) = st.borrow_mut().as_mut() {
            f(&mut s.ui);
        }
    });
}

fn stop_recording(hwnd: HWND) {
    hook::stop_recording();
    set_ui(|u| u.recording = false);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn apply(hwnd: HWND, action: Action) {
    match action {
        Action::RecordHotkey => {
            // Clicar de novo na linha cancela a gravação.
            let recording = STATE
                .with(|st| st.borrow().as_ref().map(|s| s.ui.recording))
                .unwrap_or(false);
            if recording {
                stop_recording(hwnd);
            } else {
                set_ui(|u| {
                    u.recording = true;
                    u.error = None;
                });
                hook::start_recording(hwnd);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            return;
        }
        Action::ResetHotkey => {
            settings::update(|st| st.hotkey = hotkey::DEFAULT);
            hook::set_hotkey(hotkey::DEFAULT);
            set_ui(|u| u.error = None);
        }
        _ => {
            settings::update(|st| match action {
                Action::Mode(m) => st.mode = m,
                Action::Theme(t) => st.theme = t,
                Action::Lang(l) => st.lang = l,
                Action::ToggleAutostart => st.autostart = !st.autostart,
                Action::RecordHotkey | Action::ResetHotkey => {}
            });
            if action == Action::ToggleAutostart {
                autostart::set(settings::get().autostart);
            }
        }
    }
    unsafe {
        let title = to_wide(i18n::t(Label::OptionsTitle));
        let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
        style_frame(hwnd);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    notify_main();
}

/// Resultado da gravação: valida e aplica (ou mostra o erro).
fn hotkey_recorded(hwnd: HWND, vk: u16, mods: u32) {
    set_ui(|u| u.recording = false);
    if vk == hotkey::VK_ESCAPE && mods == 0 {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        return;
    }
    let hk = Hotkey { alt: mods & 1 != 0, ctrl: mods & 2 != 0, shift: mods & 4 != 0, vk };
    match hk.validate() {
        Ok(()) => {
            settings::update(|st| st.hotkey = hk);
            hook::set_hotkey(hk);
            set_ui(|u| u.error = None);
            notify_main();
        }
        Err(e) => set_ui(|u| {
            u.error = Some(match e {
                Invalid::NeedsModifier => Label::HotkeyNeedsModifier,
                Invalid::Reserved => Label::HotkeyReserved,
            })
        }),
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
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
        let (scale, focus, ui, has_focus) = (st.scale, st.focus, st.ui, GetFocus() == hwnd);
        let Some(t) = st.target.as_mut() else { return };
        t.resize(w, h);

        let p = render::palette();
        let lay = layout(scale, ui);
        let radios = radio_indices(&lay.items);
        t.begin(p.opt_bg);

        for card in &lay.cards {
            t.rrect(*card, 10.0 * scale, p.opt_card);
        }
        for (n, it) in lay.items.iter().enumerate() {
            let r = it.rect;
            match it.kind {
                Kind::Header => {
                    t.text(&it.text, r, 12.5 * scale, 600, Align::Left, p.muted);
                    continue;
                }
                Kind::Note => {
                    t.text(&it.text, r, 12.5 * scale, 400, Align::Left, (230, 80, 70, 255));
                    continue;
                }
                Kind::Row => {}
            }
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
            let text_right = r.right - px(scale, if it.value.is_some() { 16.0 } else { 44.0 });
            let tr = RECT { left: r.left + px(scale, 16.0), right: text_right, ..r };
            t.text(&it.text, tr, 14.0 * scale, 400, Align::Left, p.text);

            if let Some(v) = &it.value {
                // Valor à direita (o atalho atual ou "Pressione as teclas...").
                let vr = RECT { left: r.left + px(scale, 200.0), right: text_right, ..r };
                let color = if it.value_accent { p.accent } else { p.muted };
                t.text(v, vr, 14.0 * scale, if it.value_accent { 600 } else { 500 }, Align::Right, color);
            }
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

fn current_ui() -> (f32, Ui) {
    STATE
        .with(|st| st.borrow().as_ref().map(|s| (s.scale, s.ui)))
        .unwrap_or((1.0, Ui::default()))
}

fn activate_at(hwnd: HWND, list_index: usize) {
    let (scale, ui) = current_ui();
    let lay = layout(scale, ui);
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
        hook::WM_HOTKEY_RECORDED => {
            hotkey_recorded(hwnd, wp.0 as u16, lp.0 as u32);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
            let (scale, ui) = current_ui();
            let lay = layout(scale, ui);
            let radios = radio_indices(&lay.items);
            let hit = lay.items.iter().position(|i| {
                i.action.is_some()
                    && x >= i.rect.left
                    && x < i.rect.right
                    && y >= i.rect.top
                    && y < i.rect.bottom
            });
            match hit {
                Some(n) => {
                    if let Some(f) = radios.iter().position(|&r| r == n) {
                        STATE.with(|st| {
                            if let Some(s) = st.borrow_mut().as_mut() {
                                s.focus = f;
                            }
                        });
                    }
                    let _ = SetFocus(Some(hwnd));
                    activate_at(hwnd, n);
                }
                None => {
                    if ui.recording {
                        stop_recording(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = VIRTUAL_KEY(wp.0 as u16);
            let (scale, ui) = current_ui();
            let radios = radio_indices(&layout(scale, ui).items);
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
                        activate_at(hwnd, n);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            // Perder o foco cancela uma gravação em andamento.
            if current_ui().1.recording {
                stop_recording(hwnd);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_SETFOCUS => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_DESTROY => {
            hook::stop_recording();
            HWND_OPT.with(|c| c.set(0));
            STATE.with(|st| *st.borrow_mut() = None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
