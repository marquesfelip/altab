//! Ícone da bandeja e seu menu (Opções / Sair).

use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT, WPARAM, LPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::i18n::{self, Label};
use crate::windows_list::to_wide;

pub const WM_TRAY: u32 = WM_APP + 10;
const ICON_ID: u32 = 1;
const CMD_OPTIONS: usize = 1;
const CMD_EXIT: usize = 2;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub enum Command {
    Options,
    Exit,
}

/// Mensagem que o Explorer envia quando a barra de tarefas é recriada.
pub fn taskbar_created_msg() -> u32 {
    TASKBAR_CREATED.load(Ordering::Relaxed)
}

pub fn add(hwnd: HWND) {
    unsafe {
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
    }
    notify(hwnd, NIM_ADD);
}

pub fn update(hwnd: HWND) {
    notify(hwnd, NIM_MODIFY);
}

pub fn remove(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn notify(hwnd: HWND, op: NOTIFY_ICON_MESSAGE) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: app_icon(unsafe { GetSystemMetrics(SM_CXSMICON) }),
        ..Default::default()
    };
    let tip: Vec<u16> = i18n::t(Label::TrayTooltip).encode_utf16().collect();
    let n = tip.len().min(nid.szTip.len() - 1);
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    unsafe {
        let _ = Shell_NotifyIconW(op, &nid);
    }
}

/// Mostra o menu junto ao cursor e devolve o comando escolhido.
pub fn show_menu(hwnd: HWND) -> Option<Command> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let opt = to_wide(i18n::t(Label::TrayOptions));
        let exit = to_wide(i18n::t(Label::TrayExit));
        let _ = AppendMenuW(menu, MF_STRING, CMD_OPTIONS, PCWSTR(opt.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT, PCWSTR(exit.as_ptr()));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Sem isso o menu não fecha ao clicar fora dele.
        let _ = SetForegroundWindow(hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match id.0 as usize {
            CMD_OPTIONS => Some(Command::Options),
            CMD_EXIT => Some(Command::Exit),
            _ => None,
        }
    }
}

/// Ícone do app, embutido no .exe (recurso de ID 1, ver `build.rs`) no tamanho pedido.
pub fn app_icon(size: i32) -> HICON {
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else {
            return HICON::default();
        };
        LoadImageW(
            Some(inst.into()),
            PCWSTR(1usize as *const u16),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        )
        .map(|h| HICON(h.0))
        .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
    }
}
