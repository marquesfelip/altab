//! Enumeração das janelas "Alt+Tab" agrupadas por aplicativo.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;

use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, FILE_ATTRIBUTE_NORMAL,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct WinInfo {
    pub hwnd: HWND,
    pub title: String,
}

pub struct AppEntry {
    pub name: String,
    pub icon: HICON,
    /// Mais recente primeiro.
    pub windows: Vec<WinInfo>,
}

/// Caches que sobrevivem entre aberturas do seletor.
#[derive(Default)]
pub struct Caches {
    names: HashMap<String, String>,
    icons: HashMap<String, HICON>,
}

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

fn class_name(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

fn exe_path_of_pid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

unsafe extern "system" fn collect_top(hwnd: HWND, lparam: LPARAM) -> BOOL {
    (*(lparam.0 as *mut Vec<HWND>)).push(hwnd);
    BOOL(1)
}

struct ChildSearch {
    frame_pid: u32,
    found: Option<u32>,
}

unsafe extern "system" fn find_core_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let s = &mut *(lparam.0 as *mut ChildSearch);
    let pid = pid_of(hwnd);
    if pid != s.frame_pid {
        s.found = Some(pid);
        return BOOL(0);
    }
    BOOL(1)
}

/// Para apps UWP o dono real da janela é um processo filho do ApplicationFrameHost.
fn resolve_pid(hwnd: HWND, exe: &str) -> u32 {
    let pid = pid_of(hwnd);
    let is_frame_host = Path::new(exe)
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("ApplicationFrameHost.exe"));
    if !is_frame_host {
        return pid;
    }
    let mut s = ChildSearch { frame_pid: pid, found: None };
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(find_core_window), LPARAM(&mut s as *mut _ as isize));
    }
    s.found.unwrap_or(pid)
}

fn is_alt_tab_window(hwnd: HWND, own: HWND) -> bool {
    unsafe {
        if hwnd == own || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let app_window = ex & WS_EX_APPWINDOW.0 != 0;
        if !app_window && ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return false;
        }
        let has_owner = GetWindow(hwnd, GW_OWNER).is_ok_and(|h| !h.0.is_null());
        if has_owner && !app_window {
            return false;
        }
        if matches!(
            class_name(hwnd).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Windows.UI.Core.CoreWindow"
        ) {
            return false;
        }
        GetWindowTextLengthW(hwnd) > 0
    }
}

fn file_description(path: &str) -> Option<String> {
    unsafe {
        let wide = to_wide(path);
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void)
            .ok()?;

        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(
            data.as_ptr() as *const c_void,
            w!("\\VarFileInfo\\Translation"),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return None;
        }
        let tr = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key = to_wide(&format!(
            "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
            tr[0], tr[1]
        ));
        let mut vptr: *mut c_void = std::ptr::null_mut();
        let mut vlen = 0u32;
        if !VerQueryValueW(
            data.as_ptr() as *const c_void,
            PCWSTR(key.as_ptr()),
            &mut vptr,
            &mut vlen,
        )
        .as_bool()
            || vlen == 0
        {
            return None;
        }
        let s = std::slice::from_raw_parts(vptr as *const u16, vlen as usize);
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        let name = String::from_utf16_lossy(&s[..end]).trim().to_string();
        (!name.is_empty()).then_some(name)
    }
}

fn app_name(exe: &str, caches: &mut Caches) -> String {
    if let Some(n) = caches.names.get(exe) {
        return n.clone();
    }
    let name = file_description(exe).unwrap_or_else(|| {
        Path::new(exe)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| exe.to_string())
    });
    caches.names.insert(exe.to_string(), name.clone());
    name
}

fn window_icon(hwnd: HWND, exe: &str, caches: &mut Caches) -> HICON {
    unsafe {
        for kind in [ICON_BIG, ICON_SMALL2] {
            let mut res = 0usize;
            let r = SendMessageTimeoutW(
                hwnd,
                WM_GETICON,
                WPARAM(kind as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                30,
                Some(&mut res),
            );
            if r.0 != 0 && res != 0 {
                return HICON(res as *mut c_void);
            }
        }
        let cls = GetClassLongPtrW(hwnd, GCLP_HICON);
        if cls != 0 {
            return HICON(cls as *mut c_void);
        }
        if let Some(i) = caches.icons.get(exe) {
            return *i;
        }
        let wide = to_wide(exe);
        let mut info = SHFILEINFOW::default();
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        caches.icons.insert(exe.to_string(), info.hIcon);
        info.hIcon
    }
}

/// Lista os aplicativos com janelas abertas, o mais recentemente usado primeiro.
pub fn enumerate(own: HWND, caches: &mut Caches) -> Vec<AppEntry> {
    let mut tops: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_top), LPARAM(&mut tops as *mut _ as isize));
    }

    let mut apps: Vec<AppEntry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for hwnd in tops {
        if !is_alt_tab_window(hwnd, own) {
            continue;
        }
        let Some(frame_exe) = exe_path_of_pid(pid_of(hwnd)) else {
            continue;
        };
        let pid = resolve_pid(hwnd, &frame_exe);
        let exe = if pid_of(hwnd) == pid {
            frame_exe
        } else {
            exe_path_of_pid(pid).unwrap_or(frame_exe)
        };
        let key = exe.to_lowercase();
        let info = WinInfo { hwnd, title: window_text(hwnd) };
        match index.get(&key) {
            Some(&i) => apps[i].windows.push(info),
            None => {
                index.insert(key, apps.len());
                apps.push(AppEntry {
                    name: app_name(&exe, caches),
                    icon: window_icon(hwnd, &exe, caches),
                    windows: vec![info],
                });
            }
        }
    }
    apps
}
