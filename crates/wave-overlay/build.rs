//! Embed the reviewed ICO using the Windows SDK resource compiler. No new crate
//! or runtime dependency is needed; the SDK is already required for MSVC builds.
use std::{env, path::PathBuf, process::Command};

fn resource_compiler() -> PathBuf {
    if let Some(explicit) = env::var_os("RC") {
        return explicit.into();
    }
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath") {
        let candidate = PathBuf::from(bin).join("x64/rc.exe");
        if candidate.is_file() {
            return candidate;
        }
    }
    let kits = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
        .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = std::fs::read_dir(kits)
        .expect("Install the Windows SDK resource compiler or set RC")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("x64/rc.exe").is_file())
        .collect();
    versions.sort();
    versions.pop().expect("Windows SDK rc.exe not found").join("x64/rc.exe")
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/app.ico");
    println!("cargo:rerun-if-changed=assets/icon/app.rc");
    for name in ["RC", "WindowsSdkVerBinPath", "ProgramFiles(x86)"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "Windows MSVC target required"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory")).join("app-icon.res");
    let result = Command::new(resource_compiler())
        .current_dir(root)
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("assets/icon/app.rc")
        .status()
        .expect("Could not start Windows SDK resource compiler");
    assert!(result.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", output.display());
}
