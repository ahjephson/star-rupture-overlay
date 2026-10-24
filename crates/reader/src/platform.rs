use crate::{Identity, Mem};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs::File,
    io::Read,
    mem::{size_of, zeroed},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{
        Diagnostics::{Debug::ReadProcessMemory, ToolHelp::*},
        Threading::*,
    },
};
const GAME: &str = "StarRuptureGameSteam-Win64-Shipping.exe";
const EXPECTED: &str = "C:\\Program Files (x86)\\Steam\\steamapps\\common\\StarRupture\\StarRupture\\Binaries\\Win64\\StarRuptureGameSteam-Win64-Shipping.exe";
struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
#[derive(Clone)]
pub struct Process {
    pub identity: Identity,
    handle: Arc<OwnedHandle>,
}
fn checked(ok: i32) -> Result<()> {
    if ok == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
fn open(pid: u32) -> Result<Process> {
    // Only query/read permissions. No debug privilege adjustment, writes or remote threads.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    ensure!(
        !raw.is_null(),
        "Cannot open game read-only: {}",
        std::io::Error::last_os_error()
    );
    let handle = Arc::new(OwnedHandle(raw));
    let mut chars = vec![0u16; 32768];
    let mut count = chars.len() as u32;
    checked(unsafe { QueryFullProcessImageNameW(raw, 0, chars.as_mut_ptr(), &mut count) })?;
    let executable = String::from_utf16(&chars[..count as usize])?;
    ensure!(
        executable.eq_ignore_ascii_case(EXPECTED),
        "Unexpected game executable path"
    );
    let mut created = unsafe { zeroed() };
    let mut exited = unsafe { zeroed() };
    let mut kernel = unsafe { zeroed() };
    let mut user = unsafe { zeroed() };
    checked(unsafe { GetProcessTimes(raw, &mut created, &mut exited, &mut kernel, &mut user) })?;
    let creation_ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    let mut file = File::open(&executable)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let p = Process {
        identity: Identity {
            pid,
            creation_ticks,
            executable,
            executable_sha256: format!("{:x}", hash.finalize()),
        },
        handle,
    };
    p.ensure_alive()?;
    Ok(p)
}
impl Process {
    pub fn ensure_alive(&self) -> Result<()> {
        let mut code = 0;
        checked(unsafe { GetExitCodeProcess(self.handle.0, &mut code) })?;
        ensure!(code == 259, "Game process exited");
        Ok(())
    }
}
pub fn find_game() -> Result<Process> {
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    ensure!(raw != INVALID_HANDLE_VALUE, "Cannot enumerate processes");
    let snapshot = OwnedHandle(raw);
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ids = vec![];
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while ok != 0 {
        let n = entry
            .szExeFile
            .iter()
            .position(|x| *x == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..n]).eq_ignore_ascii_case(GAME) {
            ids.push(entry.th32ProcessID);
        }
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    if ids.is_empty() {
        bail!("Game is not running")
    }
    ensure!(
        ids.len() == 1,
        "Multiple game processes; refusing ambiguous session"
    );
    open(ids[0]).context("Opening installed StarRupture")
}
#[async_trait::async_trait(?Send)]
impl Mem for Process {
    async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
        ensure!(
            (0x10000..0x7fffffffffff).contains(&address) && buf.len() <= 16 * 1024 * 1024,
            "Invalid bounded memory read"
        );
        address
            .checked_add(buf.len() as u64)
            .context("Address overflow")?;
        let mut got = 0;
        checked(unsafe {
            ReadProcessMemory(
                self.handle.0,
                address as *const c_void,
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut got,
            )
        })?;
        ensure!(got == buf.len(), "Partial read");
        Ok(())
    }
    // Mem's default write method fails; this handle has no write permissions.
}
pub fn is_elevated() -> Result<bool> {
    let mut raw = std::ptr::null_mut();
    checked(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) })?;
    let h = OwnedHandle(raw);
    let mut e: TOKEN_ELEVATION = unsafe { zeroed() };
    let mut returned = 0;
    checked(unsafe {
        GetTokenInformation(
            h.0,
            TokenElevation,
            (&mut e as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    })?;
    Ok(e.TokenIsElevated != 0)
}
