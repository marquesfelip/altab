//! Estado do seletor, janela principal, ativação e preview (DWM thumbnail).

use std::cell::RefCell;
use std::ffi::c_void;

use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::fuzzy::{self, Match};
use crate::hook;
use crate::options;
use crate::settings::{self, Mode};
use crate::tray;
use crate::gfx::Target;
use crate::render::{self, Layout};
use crate::windows_list::{self, AppEntry, Caches};

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub struct App {
    pub hwnd: HWND,
    pub entries: Vec<AppEntry>,
    /// Apps que casam com a pesquisa, do mais para o menos relevante.
    pub filtered: Vec<Match>,
    pub query: String,
    pub sel: usize,
    pub scroll: usize,
    pub sticky: bool,
    pub layout: Layout,
    pub target: Option<Target>,
    pub acrylic: bool,
    thumb: isize,
    caches: Caches,
    last_mouse: POINT,
    /// Pesquisa pré-preenchida na próxima abertura (flag `--query`).
    initial_query: Option<String>,
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| {
        let mut b = a.try_borrow_mut().ok()?;
        b.as_mut().map(f)
    })
}

/// Define a pesquisa inicial da próxima abertura (uso: `--query`, depuração).
pub fn set_initial_query(q: String) {
    with_app(|a| a.initial_query = Some(q));
}

pub fn create_window() -> Result<HWND> {
    unsafe {
        let inst = GetModuleHandleW(None)?;
        let class = w!("AltabSwitcher");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            hIcon: crate::tray::app_icon(GetSystemMetrics(SM_CXICON)),
            hIconSm: crate::tray::app_icon(GetSystemMetrics(SM_CXSMICON)),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("altab"),
            WS_POPUP,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(inst.into()),
            None,
        )?;

        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const _ as *const c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                entries: Vec::new(),
                filtered: Vec::new(),
                query: String::new(),
                sel: 0,
                scroll: 0,
                sticky: false,
                layout: Layout::new(1.0),
                target: None,
                acrylic: false,
                thumb: 0,
                caches: Caches::default(),
                last_mouse: POINT::default(),
                initial_query: None,
            });
        });
        Ok(hwnd)
    }
}

impl App {
    fn open(&mut self, reverse: bool) {
        self.query = self.initial_query.take().unwrap_or_default();
        self.sticky = settings::get().mode == Mode::Fixed;
        self.entries = windows_list::enumerate(self.hwnd, &mut self.caches);
        if let Some(t) = &self.target {
            t.clear_icons();
        }
        self.apply_filter();
        if self.entries.is_empty() {
            self.hide();
            return;
        }
        let n = self.filtered.len();
        self.sel = if reverse { n - 1 } else { 1.min(n - 1) };
        self.scroll = 0;
        self.ensure_visible();
        self.show();
    }

    fn show(&mut self) {
        unsafe {
            let fg = GetForegroundWindow();
            let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(mon, &mut mi);

            let dpi = GetDpiForSystem();
            let scale = dpi as f32 / 96.0;
            if (scale - self.layout.scale).abs() > f32::EPSILON {
                self.layout = Layout::new(scale);
            }
            let (w, h) = (self.layout.width(), self.layout.height());
            let work = mi.rcWork;
            let x = work.left + ((work.right - work.left) - w) / 2;
            let y = work.top + ((work.bottom - work.top) - h) / 2;

            let dark = settings::effective_dark();
            render::apply_frame(self.hwnd, dark, Some(if dark { 0x0040_4040 } else { 0x00D0_D0D0 }));
            self.acrylic = render::apply_backdrop(self.hwnd, dark);

            let _ = GetCursorPos(&mut self.last_mouse);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            self.update_thumbnail();
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn hide(&mut self) {
        self.drop_thumbnail();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.sticky = false;
        hook::set_open(false);
    }

    fn apply_filter(&mut self) {
        self.filtered = fuzzy::rank(&self.query, &self.entries);
        self.sel = 0;
        self.scroll = 0;
    }

    pub fn selected(&self) -> Option<&AppEntry> {
        self.filtered.get(self.sel).map(|m| &self.entries[m.app])
    }

    /// Janela casada do item selecionado (a mais recente, se o nome casou).
    pub fn selected_window(&self) -> Option<&windows_list::WinInfo> {
        let m = self.filtered.get(self.sel)?;
        self.entries[m.app].windows.get(m.window)
    }

    /// Total de janelas visíveis na lista filtrada.
    pub fn window_count(&self) -> usize {
        self.filtered.iter().map(|m| self.entries[m.app].windows.len()).sum()
    }

    fn ensure_visible(&mut self) {
        let vis = self.layout.visible_items();
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + vis {
            self.scroll = self.sel + 1 - vis;
        }
    }

    fn select(&mut self, idx: usize) {
        if idx != self.sel && idx < self.filtered.len() {
            self.sel = idx;
            self.ensure_visible();
            self.update_thumbnail();
            self.redraw();
        }
    }

    fn nav(&mut self, delta: isize) {
        let n = self.filtered.len() as isize;
        if n == 0 {
            return;
        }
        let idx = (self.sel as isize + delta).rem_euclid(n) as usize;
        self.select(idx);
    }

    fn redraw(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn drop_thumbnail(&mut self) {
        if self.thumb != 0 {
            unsafe {
                let _ = DwmUnregisterThumbnail(self.thumb);
            }
            self.thumb = 0;
        }
    }

    fn update_thumbnail(&mut self) {
        self.drop_thumbnail();
        let Some(target) = self.selected_window().map(|w| w.hwnd) else {
            return;
        };
        unsafe {
            let Ok(t) = DwmRegisterThumbnail(self.hwnd, target) else {
                return;
            };
            self.thumb = t;
            let area = self.layout.thumb_rect();
            let aw = (area.right - area.left) as f32;
            let ah = (area.bottom - area.top) as f32;
            let src = DwmQueryThumbnailSourceSize(t).unwrap_or_default();
            let dest = if src.cx > 0 && src.cy > 0 {
                let k = (aw / src.cx as f32).min(ah / src.cy as f32).min(1.0 * self.layout.scale.max(1.0));
                let (dw, dh) = (src.cx as f32 * k, src.cy as f32 * k);
                let l = area.left + ((aw - dw) / 2.0) as i32;
                let tp = area.top + ((ah - dh) / 2.0) as i32;
                RECT { left: l, top: tp, right: l + dw as i32, bottom: tp + dh as i32 }
            } else {
                area
            };
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
                rcDestination: dest,
                opacity: 255,
                fVisible: true.into(),
                ..Default::default()
            };
            let _ = DwmUpdateThumbnailProperties(t, &props);
        }
    }

    fn activate_selected(&mut self) {
        let target = self.selected_window().map(|w| w.hwnd);
        self.hide();
        if let Some(h) = target {
            activate(h);
        }
    }

    fn paint(&mut self) {
        render::paint(self);
    }

    fn mouse_move(&mut self, x: i32, y: i32) {
        let p = unsafe {
            let mut p = POINT { x, y };
            let _ = ClientToScreen(self.hwnd, &mut p);
            p
        };
        // O Windows gera um WM_MOUSEMOVE sintético ao exibir a janela sob o cursor.
        if p.x == self.last_mouse.x && p.y == self.last_mouse.y {
            return;
        }
        self.last_mouse = p;
        if let Some(i) = self.layout.item_at(x, y, self.scroll) {
            self.select(i);
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        if let Some(i) = self.layout.item_at(x, y, self.scroll) {
            if i < self.filtered.len() {
                self.sel = i;
                self.activate_selected();
            }
        }
    }
}

/// Traz a janela para a frente, restaurando-a se estiver minimizada.
fn activate(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let me = GetCurrentThreadId();
        let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attach = fg_tid != 0 && fg_tid != me;
        if attach {
            let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg_tid, true);
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attach {
            let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg_tid, false);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let xy = |lp: LPARAM| ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
    match msg {
        hook::WM_OPEN => {
            with_app(|a| a.open(wp.0 != 0));
            LRESULT(0)
        }
        hook::WM_NAV => {
            with_app(|a| a.nav(lp.0));
            LRESULT(0)
        }
        hook::WM_CHAR_IN => {
            with_app(|a| {
                if let Some(c) = char::from_u32(wp.0 as u32) {
                    a.sticky = true;
                    a.query.push(c);
                    a.apply_filter();
                    a.update_thumbnail();
                    a.redraw();
                }
            });
            LRESULT(0)
        }
        hook::WM_BACK => {
            with_app(|a| {
                a.sticky = true;
                a.query.pop();
                a.apply_filter();
                a.update_thumbnail();
                a.redraw();
            });
            LRESULT(0)
        }
        hook::WM_ENTER => {
            with_app(|a| a.activate_selected());
            LRESULT(0)
        }
        hook::WM_CANCEL => {
            with_app(|a| a.hide());
            LRESULT(0)
        }
        hook::WM_MOD_UP => {
            with_app(|a| {
                if !a.sticky && a.thumb_or_open() {
                    a.activate_selected();
                }
            });
            LRESULT(0)
        }
        WM_PAINT => {
            if with_app(|a| a.paint()).is_none() {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            LRESULT(0)
        }
        tray::WM_TRAY => {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == WM_LBUTTONUP || ev == WM_RBUTTONUP {
                match tray::show_menu(hwnd) {
                    Some(tray::Command::Options) => options::show(hwnd),
                    Some(tray::Command::Exit) => {
                        tray::remove(hwnd);
                        PostQuitMessage(0);
                    }
                    None => {}
                }
            }
            LRESULT(0)
        }
        options::WM_SETTINGS_CHANGED => {
            tray::update(hwnd);
            with_app(|a| {
                // Em modo Fixo ligado/desligado durante o uso, vale na próxima abertura.
                if IsWindowVisible(a.hwnd).as_bool() {
                    a.redraw();
                }
            });
            LRESULT(0)
        }
        _ if msg == tray::taskbar_created_msg() && msg != 0 => {
            tray::add(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with_app(|a| a.mouse_move(x, y));
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            with_app(|a| a.click(x, y));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16;
            with_app(|a| a.nav(if delta > 0 { -1 } else { 1 }));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl App {
    /// O seletor só reage ao Alt solto se estiver realmente visível.
    fn thumb_or_open(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }
}
