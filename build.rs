//! Build helper for the GNU Rust toolchain (see rust-toolchain.toml).
//!
//! Its bundled MinGW libraries have no `libshlwapi.a`, which a dependency (webbrowser) links.
//! The `windows_x86_64_gnu` crate ships `libwindows.0.52.0.a`, an import library covering every
//! Windows DLL, so a copy of it named `libshlwapi.a` is placed in OUT_DIR and added to the search path.

use std::path::{Path, PathBuf};

fn cargo_home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("CARGO_HOME") {
        return Some(PathBuf::from(h));
    }
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".cargo"))
}

fn find_umbrella_lib() -> Option<PathBuf> {
    let src = cargo_home()?.join("registry").join("src");
    for index in std::fs::read_dir(src).ok()?.flatten() {
        for krate in std::fs::read_dir(index.path()).ok()?.flatten() {
            let name = krate.file_name().to_string_lossy().to_string();
            if name.starts_with("windows_x86_64_gnu-0.52.") {
                let lib = krate.path().join("lib").join("libwindows.0.52.0.a");
                if lib.exists() {
                    return Some(lib);
                }
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return; // MSVC builds get shlwapi.lib from the Windows SDK
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let dest = out.join("libshlwapi.a");
    if !dest.exists() {
        match find_umbrella_lib() {
            Some(lib) => {
                std::fs::copy(&lib, &dest).expect("copy import library");
            }
            None => {
                println!("cargo:warning=libwindows.0.52.0.a not found; linking shlwapi may fail");
                return;
            }
        }
    }
    println!("cargo:rustc-link-search=native={}", Path::new(&out).display());
}
