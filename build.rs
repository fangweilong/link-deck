fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("linkdeck.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: failed to compile winres resource: {}", e);
        }
    }
}
