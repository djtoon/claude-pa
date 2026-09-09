//! Embeds the pa-tray binary (built separately, see build.sh / the GitHub workflow) so the installer
//! can drop it into `<project>/.pa/bin/`. If it is not found the installer still builds, without the tray app.

use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let exe = if target_os == "windows" { "pa-tray.exe" } else { "pa-tray" };

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = env::var("PA_TRAY_BIN") {
        candidates.push(PathBuf::from(p));
    }
    if let Ok(t) = env::var("TARGET") {
        candidates.push(manifest.join("../pa-tray/target").join(&t).join("release").join(exe));
    }
    candidates.push(manifest.join("../pa-tray/target/release").join(exe));

    let dest = out.join("pa-tray.bin");
    match candidates.iter().find(|p| p.is_file()) {
        Some(p) => {
            fs::copy(p, &dest).expect("copy pa-tray");
            println!("cargo:warning=embedding tray app from {}", p.display());
        }
        None => {
            fs::write(&dest, b"").expect("write empty tray placeholder");
            println!("cargo:warning=pa-tray binary not found: build pa-tray first (cargo build --release --manifest-path ../pa-tray/Cargo.toml) or set PA_TRAY_BIN. Installer ships without the tray app.");
        }
    }
    fs::write(out.join("pa-tray.name"), exe).expect("write tray name");
    println!("cargo:rerun-if-env-changed=PA_TRAY_BIN");
    for c in &candidates {
        println!("cargo:rerun-if-changed={}", c.display());
    }
}
