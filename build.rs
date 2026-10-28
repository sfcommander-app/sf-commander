use std::io;

fn main() -> io::Result<()> {
    // Re-run when the version or the icon changes, so that the numbers in
    // the .exe properties never lag behind Cargo.toml
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = env!("CARGO_PKG_VERSION");
        let shown = version.strip_suffix(".0").unwrap_or(version);
        let mut resource = winres::WindowsResource::new();
        if cfg!(unix) && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
            resource.set_windres_path("x86_64-w64-mingw32-windres")
                .set_ar_path("x86_64-w64-mingw32-ar");
        }
        resource
            .set_icon("assets/icon.ico")
            // The visible name. The file names (`SF-Commander-*.exe`, the
            // handover file, the update manifest) keep the old spelling:
            // every copy already out there only accepts those, so a
            // renamed exe would never reach anybody through the updater
            .set("ProductName", "S&F Commander")
            .set("FileDescription", &format!("S&F Commander {shown}"))
            .set("ProductVersion", version)
            .set("FileVersion", version)
            .set("CompanyName", "GussOlean (Petr Macinka)")
            .set("OriginalFilename", "SF-Commander.exe")
            .set("LegalCopyright", "MIT - based on sf-scrapbook-helper")
            .compile()?;
    }
    Ok(())
}
