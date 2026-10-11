//! Início automático com o Windows via `HKCU\...\Run` (sem necessidade de admin).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("altab");

/// Liga (grava o caminho do exe atual) ou desliga o início automático.
pub fn set(enabled: bool) {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
            return;
        }
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                // O caminho vai entre aspas para suportar espaços.
                let cmd = format!("\"{}\"", exe.display());
                let wide: Vec<u8> = cmd
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(|u| u.to_le_bytes())
                    .collect();
                let _ = RegSetValueExW(key, VALUE, None, REG_SZ, Some(&wide));
            }
        } else {
            let _ = RegDeleteValueW(key, VALUE);
        }
        let _ = RegCloseKey(key);
    }
}
