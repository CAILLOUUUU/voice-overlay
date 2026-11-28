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
        // Compilation croisée MSVC sans llvm-rc : on essaie windres (MinGW), qui sait produire un .res.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") && windres_fallback(&version) {
            return;
        }
        // Sinon le .exe marche quand même, juste sans icône.
        println!("cargo:warning=ressources Windows non ajoutées : {e}");
    }
}

fn windres_fallback(version: &str) -> bool {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = std::fs::canonicalize("../icon.ico").unwrap();
    let nums = version.replace('.', ",") + ",0";
    let rc = format!(
        "1 ICON \"{}\"\n1 VERSIONINFO\nFILEVERSION {nums}\nPRODUCTVERSION {nums}\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"040904b0\"\n  BEGIN\n   VALUE \"ProductName\", \"Voice Overlay - Caillou feat Claude\"\n   VALUE \"FileDescription\", \"Voice Overlay - Caillou feat Claude\"\n   VALUE \"CompanyName\", \"Caillou\"\n   VALUE \"InternalName\", \"VoiceOverlay\"\n   VALUE \"OriginalFilename\", \"Voice-Overlay-{version}.exe\"\n   VALUE \"LegalCopyright\", \"Caillou feat Claude\"\n   VALUE \"FileVersion\", \"{version}\"\n   VALUE \"ProductVersion\", \"{version}\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \"Translation\", 0x409, 1200\n END\nEND\n",
        ico.display().to_string().replace('\\', "/")
    );
    let rc_path = out.join("app.rc");
    let res_path = out.join("app.res");
    if std::fs::write(&rc_path, rc).is_err() {
        return false;
    }
    for tool in ["x86_64-w64-mingw32-windres", "windres"] {
        let ok = std::process::Command::new(tool)
            .args(["-O", "res", "-i"])
            .arg(&rc_path)
            .arg("-o")
            .arg(&res_path)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
            return true;
        }
    }
    false
}
