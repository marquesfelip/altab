#![windows_subsystem = "windows"]

mod gfx;
mod autostart;
mod fuzzy;
mod hook;
mod i18n;
mod options;
mod render;
mod settings;
mod switcher;
mod tray;
mod windows_list;

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostMessageW, TranslateMessage, MSG,
};

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Instância única: o mutex vive até o processo terminar.
        let _mutex = CreateMutexW(None, true, w!("Local\\altab-single-instance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let Ok(hwnd) = switcher::create_window() else {
            return;
        };
        if hook::install(hwnd).is_err() {
            return;
        }
        tray::add(hwnd);

        // Mantém a entrada de início automático em sincronia com a opção (e com o
        // caminho atual do exe). Execuções de depuração não mexem no registro.
        let debug = std::env::args().any(|a| matches!(a.as_str(), "--show" | "--options" | "--query"));
        if !debug {
            autostart::set(settings::get().autostart);
        }

        if std::env::args().any(|a| a == "--options") {
            options::show(hwnd);
        }

        // `--show`: abre o seletor já na inicialização (útil para depurar o visual).
        let args: Vec<String> = std::env::args().collect();
        if let Some(q) = args.iter().position(|a| a == "--query").and_then(|i| args.get(i + 1)) {
            switcher::set_initial_query(q.clone());
        }
        if args.iter().any(|a| a == "--show") {
            hook::set_open(true);
            let _ = PostMessageW(Some(hwnd), hook::WM_OPEN, WPARAM(0), LPARAM(0));
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
