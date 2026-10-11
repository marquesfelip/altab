// Embute ícone e metadados de versão no .exe (somente no Windows).
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/altab.ico");
        res.set("ProductName", "altab");
        res.set("FileDescription", "altab - Alt+Tab switcher");
        res.set("LegalCopyright", "Copyright (c) 2026 marquesfelip - MIT License");
        res.set("OriginalFilename", "altab.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=nao foi possivel embutir os recursos do Windows: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon/altab.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
