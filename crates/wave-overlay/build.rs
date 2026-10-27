//! Compile native dialogs, the manifest and the reviewed icon with the Windows SDK.
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn sdk_headers(compiler: &Path) -> Option<PathBuf> {
    let version = compiler.parent()?.parent()?;
    let sdk = version.parent()?.parent()?;
    let include = sdk.join("Include").join(version.file_name()?);
    (include.join("um/windows.h").is_file() && include.join("shared").is_dir()).then_some(include)
}

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
    for path in ["assets/app.rc", "assets/dialogs.rc", "assets/version.rc", "assets/app.manifest"] {
        println!("cargo:rerun-if-changed={path}");
    }
    let revision = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=OVERLAY_REVISION={revision}");
    let mut refs = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Ok(output) = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).output()
        && output.status.success()
    {
        refs.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    for name in refs {
        let args = ["rev-parse", "--git-path", &name];
        if let Ok(output) = Command::new("git").args(args).output()
            && output.status.success()
        {
            println!("cargo:rerun-if-changed={}", String::from_utf8_lossy(&output.stdout).trim());
        }
    }
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
    let output_directory = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    assert!(version.bytes().all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c)));
    let components = ["MAJOR", "MINOR", "PATCH"].map(|part| {
        env::var(format!("CARGO_PKG_VERSION_{part}"))
            .expect("Cargo version component")
            .parse::<u16>()
            .expect("Windows version components must fit in 16 bits")
    });
    std::fs::write(
        output_directory.join("overlay-version.h"),
        format!(
            "#define OVERLAY_VERSION {},{},{},0\n#define OVERLAY_VERSION_STRING \"{}\\0\"\n",
            components[0], components[1], components[2], version
        ),
    )
    .expect("Could not write Windows version header");
    let output = output_directory.join("app-icon.res");
    let compiler = resource_compiler();
    let mut command = Command::new(&compiler);
    command.arg("/I").arg(&output_directory);
    if let Some(headers) = sdk_headers(&compiler) {
        command.arg("/I").arg(headers.join("um")).arg("/I").arg(headers.join("shared"));
    }
    let result = command
        .current_dir(root)
        .arg("/nologo")
        .arg("/c65001")
        .arg("/fo")
        .arg(&output)
        .arg("assets/app.rc")
        .status()
        .expect("Could not start Windows SDK resource compiler");
    assert!(result.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", output.display());
}
