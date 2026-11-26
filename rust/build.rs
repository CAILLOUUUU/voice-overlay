// Icône et informations de version dans le .exe (Windows uniquement).
fn main() {
    println!("cargo:rerun-if-changed=../icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../icon.ico")
        .set("ProductName", "Voice Overlay - Caillou feat Claude")
        .set("FileDescription", "Voice Overlay - Caillou feat Claude")
        .set("CompanyName", "Caillou")
        .set("InternalName", "VoiceOverlay")
        .set("OriginalFilename", &format!("Voice-Overlay-{version}.exe"))
        .set("LegalCopyright", "Caillou feat Claude");
    if let Err(e) = res.compile() {
        // Sans windres (compilation croisée incomplète), le .exe marche quand même, juste sans icône.
        println!("cargo:warning=ressources Windows non ajoutées : {e}");
    }
}
