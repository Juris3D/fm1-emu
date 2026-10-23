// The Windows .exe gets the app's icon (assets/icon.ico) and its name in the version information.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "FM-1 SMK-37 Desk");
            res.set("FileDescription", "FM-1 SMK-37 Desk: the M-VAVE FM-1 and SMK-37, emulated");
            res.set("LegalCopyright", "MIT licence. Emulator: fm1-emu by Joakim Eriksson. Firmwares: their authors.");
            if let Err(e) = res.compile() {
                println!("cargo:warning=no icon in the .exe: {e}");
            }
        }
    }
}
