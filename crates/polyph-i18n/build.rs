//! Embeds every `locales/<lang>/*.ftl` file, so adding a locale is a matter
//! of adding a folder.

use std::fmt::Write;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("../../locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from("pub static LOCALES: &[(&str, &[&str])] = &[\n");
    let mut langs: Vec<_> =
        std::fs::read_dir(&root).map(|r| r.flatten().filter(|e| e.path().is_dir()).collect()).unwrap_or_default();
    langs.sort_by_key(|e| e.file_name());
    for lang in langs {
        let name = lang.file_name().to_string_lossy().into_owned();
        let _ = write!(out, "    (\"{name}\", &[");
        let mut files: Vec<_> = std::fs::read_dir(lang.path())
            .map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "ftl")).collect())
            .unwrap_or_default();
        files.sort_by_key(|e| e.file_name());
        for f in files {
            println!("cargo:rerun-if-changed={}", f.path().display());
            let _ =
                write!(out, "include_str!({:?}), ", f.path().canonicalize().unwrap_or(f.path()).display().to_string());
        }
        out.push_str("]),\n");
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("locales.rs");
    if let Err(e) = std::fs::write(&dest, out) {
        panic!("cannot write {}: {e}", dest.display());
    }
}
