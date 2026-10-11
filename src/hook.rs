//! Hooks de baixo nível: capturam o atalho (Alt+Tab por padrão) antes do Windows
//! e, enquanto o seletor está aberto, todo o teclado. O callback só posta mensagens para a
//! janela principal; a lógica fica em `switcher`.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::hotkey::{self, Hotkey};

pub const WM_OPEN: u32 = WM_APP + 1; // wparam = 1 → sentido inverso
pub const WM_NAV: u32 = WM_APP + 2; // lparam = delta (+1 / -1)
pub const WM_CHAR_IN: u32 = WM_APP + 3; // wparam = unidade UTF-16
pub const WM_BACK: u32 = WM_APP + 4;
pub const WM_ENTER: u32 = WM_APP + 5;
pub const WM_CANCEL: u32 = WM_APP + 6;
/// Um modificador do atalho foi solto (Alt, por padrão).
pub const WM_MOD_UP: u32 = WM_APP + 7;
/// Enviada à janela de Opções durante a gravação: wparam = vk, lparam = máscara
/// de modificadores (1 = Alt, 2 = Ctrl, 4 = Shift).
pub const WM_HOTKEY_RECORDED: u32 = WM_APP + 20;

const LLKHF_ALTDOWN: u32 = 0x20;
const LLKHF_INJECTED: u32 = 0x10;

static OPEN: AtomicBool = AtomicBool::new(false);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// Depuração (`--accept-injected`): processa também teclas injetadas, para testes automatizados.
static ACCEPT_INJECTED: AtomicBool = AtomicBool::new(false);
static HOTKEY: AtomicU32 = AtomicU32::new(hotkey::DEFAULT.pack());

// Gravação de atalho: enquanto `RECORD_HWND != 0` o hook captura todas as teclas.
static RECORD_HWND: AtomicIsize = AtomicIsize::new(0);
static RECORD_MODS: AtomicU32 = AtomicU32::new(0);
static RECORD_DEADLINE: AtomicU64 = AtomicU64::new(0);
/// Segurança: a gravação expira sozinha, para nunca deixar o teclado preso.
const RECORD_TIMEOUT_MS: u64 = 10_000;

pub fn accept_injected(on: bool) {
    ACCEPT_INJECTED.store(on, Ordering::SeqCst);
}

pub fn set_hotkey(hk: Hotkey) {
    HOTKEY.store(hk.pack(), Ordering::SeqCst);
}

fn hotkey() -> Hotkey {
    Hotkey::unpack(HOTKEY.load(Ordering::SeqCst))
}

/// Passa a capturar o próximo atalho digitado e o envia para `hwnd`.
pub fn start_recording(hwnd: HWND) {
    RECORD_MODS.store(0, Ordering::SeqCst);
    let deadline = unsafe { GetTickCount64() } + RECORD_TIMEOUT_MS;
    RECORD_DEADLINE.store(deadline, Ordering::SeqCst);
    RECORD_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
}

pub fn stop_recording() {
    RECORD_HWND.store(0, Ordering::SeqCst);
}

pub fn set_open(open: bool) {
    OPEN.store(open, Ordering::SeqCst);
}

pub fn install(hwnd: HWND) -> windows::core::Result<()> {
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    unsafe {
        // Os handles ficam instalados até o processo encerrar.
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)?;
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0)?;
    }
    Ok(())
}

fn post(msg: u32, wparam: usize, lparam: isize) {
    let h = MAIN_HWND.load(Ordering::SeqCst);
    unsafe {
        let _ = PostMessageW(
            Some(HWND(h as *mut _)),
            msg,
            WPARAM(wparam),
            LPARAM(lparam),
        );
    }
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

/// Tecla "sem significado" enviada entre Alt↓ e Alt↑ para que o app em foco
/// não interprete o toque isolado de Alt como ativação da barra de menus.
fn inject_dummy_key() {
    let make = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0xE8),
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [make(KEYBD_EVENT_FLAGS(0)), make(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Converte a tecla em texto ignorando Alt/Ctrl (só Shift e CapsLock contam).
fn vk_to_char(vk: u32, scan: u32) -> Option<u16> {
    unsafe {
        let mut state = [0u8; 256];
        if key_down(VK_SHIFT) {
            state[VK_SHIFT.0 as usize] = 0x80;
        }
        if GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0 {
            state[VK_CAPITAL.0 as usize] = 1;
        }
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let layout = GetKeyboardLayout(tid);
        let mut buf = [0u16; 4];
        // Flag 0x4: não altera o estado de teclas mortas do teclado.
        let n = ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, Some(layout));
        if n == 1 && !char::from_u32(buf[0] as u32)?.is_control() {
            Some(buf[0])
        } else {
            None
        }
    }
}

/// Trata uma tecla durante a gravação de atalho. Sempre engole o evento.
fn record_key(vk: VIRTUAL_KEY, down: bool, up: bool) {
    let bit = match vk {
        VK_LMENU | VK_RMENU | VK_MENU => 1,
        VK_LCONTROL | VK_RCONTROL | VK_CONTROL => 2,
        VK_LSHIFT | VK_RSHIFT | VK_SHIFT => 4,
        VK_LWIN | VK_RWIN => 8, // engolida, mas não faz parte do atalho
        _ => 0,
    };
    if bit != 0 {
        if down {
            RECORD_MODS.fetch_or(bit, Ordering::SeqCst);
        } else if up {
            RECORD_MODS.fetch_and(!bit, Ordering::SeqCst);
        }
        return;
    }
    if down {
        let target = RECORD_HWND.swap(0, Ordering::SeqCst);
        if target != 0 {
            let mods = (RECORD_MODS.load(Ordering::SeqCst) & 7) as isize;
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as *mut _)),
                    WM_HOTKEY_RECORDED,
                    WPARAM(vk.0 as usize),
                    LPARAM(mods),
                );
            }
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let flags = k.flags.0;
        // Teclas injetadas (inclusive as nossas) são ignoradas, exceto em modo de depuração.
        let real = flags & LLKHF_INJECTED == 0
            || (ACCEPT_INJECTED.load(Ordering::SeqCst) && k.vkCode != 0xE8);
        if real {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            let vk = VIRTUAL_KEY(k.vkCode as u16);

            // Modo de gravação do atalho (janela de Opções).
            if RECORD_HWND.load(Ordering::SeqCst) != 0 {
                if GetTickCount64() > RECORD_DEADLINE.load(Ordering::SeqCst) {
                    stop_recording();
                } else {
                    record_key(vk, down, up);
                    return LRESULT(1);
                }
            }

            let hk = hotkey();
            let shift = key_down(VK_SHIFT);
            let alt = flags & LLKHF_ALTDOWN != 0 || key_down(VK_MENU);
            let ctrl = key_down(VK_CONTROL);

            if !OPEN.load(Ordering::SeqCst) {
                // Shift sem fazer parte do atalho apenas inverte o sentido.
                let shift_ok = !hk.shift || shift;
                if vk.0 == hk.vk && alt == hk.alt && ctrl == hk.ctrl && shift_ok {
                    if down {
                        OPEN.store(true, Ordering::SeqCst);
                        if hk.alt {
                            inject_dummy_key();
                        }
                        post(WM_OPEN, (shift && !hk.shift) as usize, 0);
                    }
                    return LRESULT(1);
                }
            } else {
                match vk {
                    VK_LMENU | VK_RMENU | VK_MENU => {
                        if up && hk.alt {
                            post(WM_MOD_UP, 0, 0);
                        }
                    }
                    VK_LCONTROL | VK_RCONTROL | VK_CONTROL => {
                        if up && hk.ctrl {
                            post(WM_MOD_UP, 0, 0);
                        }
                    }
                    VK_LSHIFT | VK_RSHIFT | VK_SHIFT => {
                        if up && hk.shift {
                            post(WM_MOD_UP, 0, 0);
                        }
                    }
                    VK_LWIN | VK_RWIN => {}
                    _ => {
                        if down {
                            // A tecla do atalho, com os modificadores ainda pressionados,
                            // avança (ou volta, com Shift) como o Tab no Alt+Tab.
                            let mods_held = (hk.alt || hk.ctrl)
                                && (!hk.alt || alt)
                                && (!hk.ctrl || ctrl);
                            if vk.0 == hk.vk && mods_held {
                                post(WM_NAV, 0, if shift { -1 } else { 1 });
                            } else {
                                match vk {
                                    VK_TAB => post(WM_NAV, 0, if shift { -1 } else { 1 }),
                                    VK_DOWN | VK_RIGHT => post(WM_NAV, 0, 1),
                                    VK_UP | VK_LEFT => post(WM_NAV, 0, -1),
                                    VK_RETURN => post(WM_ENTER, 0, 0),
                                    VK_ESCAPE => post(WM_CANCEL, 0, 0),
                                    VK_BACK => post(WM_BACK, 0, 0),
                                    _ => {
                                        if let Some(c) = vk_to_char(k.vkCode, k.scanCode) {
                                            post(WM_CHAR_IN, c as usize, 0);
                                        }
                                    }
                                }
                            }
                        }
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && OPEN.load(Ordering::SeqCst) {
        let msg = wparam.0 as u32;
        if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            let m = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let h = HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _);
            let mut r = RECT::default();
            if GetWindowRect(h, &mut r).is_ok() {
                let p = m.pt;
                if p.x < r.left || p.x >= r.right || p.y < r.top || p.y >= r.bottom {
                    post(WM_CANCEL, 0, 0);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
