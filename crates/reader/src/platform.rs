//! Narrow OS boundary: bounded reads through query/read-only handles.
//!
//! Buffers remain owned across each synchronous Win32 call. Handles close via
//! RAII on the reader thread; no process write, code execution or privilege APIs.
use crate::{Identity, Mem};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use starrupture_observation_model::shutdown::StopSignal;
use std::{
    ffi::c_void,
    fs::File,
    io::Read,
    mem::{size_of, zeroed},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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
    handle: Rc<OwnedHandle>,
    budget: Arc<ReadBudget>,
}
fn checked(ok: i32) -> Result<()> {
    if ok == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
pub(crate) struct ReadBudget {
    stop: Arc<StopSignal>,
    deadline: Mutex<Instant>,
}
impl ReadBudget {
    pub(crate) fn new(stop: Arc<StopSignal>) -> Arc<Self> {
        Arc::new(Self {
            stop,
            deadline: Mutex::new(Instant::now() + Duration::from_secs(60)),
        })
    }
    pub(crate) fn renew(&self, millis: u64) {
        *self.deadline.lock().unwrap_or_else(|p| p.into_inner()) =
            Instant::now() + Duration::from_millis(millis);
    }
    fn check(&self) -> Result<()> {
        ensure!(!self.stop.is_requested(), "Reader cancelled");
        ensure!(
            Instant::now() <= *self.deadline.lock().unwrap_or_else(|p| p.into_inner()),
            "Reader budget expired"
        );
        Ok(())
    }
}
fn open(pid: u32, budget: Arc<ReadBudget>) -> Result<Process> {
    budget.check()?;
    // Only query/read permissions. No debug privilege adjustment, writes or remote threads.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    ensure!(!raw.is_null(), "Cannot open game read-only: {}", std::io::Error::last_os_error());
    // Process handles stay on the reader thread; only cancellation is shared.
    let handle = Rc::new(OwnedHandle(raw));
    let mut chars = vec![0u16; 32768];
    let mut count = chars.len() as u32;
    checked(unsafe { QueryFullProcessImageNameW(raw, 0, chars.as_mut_ptr(), &mut count) })?;
    let executable = String::from_utf16(&chars[..count as usize])?;
    ensure!(executable.eq_ignore_ascii_case(EXPECTED), "Unexpected game executable path");
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
        budget.check()?;
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
        budget,
    };
    p.ensure_alive()?;
    Ok(p)
}
impl Process {
    pub fn ensure_alive(&self) -> Result<()> {
        self.budget.check()?;
        let mut code = 0;
        checked(unsafe { GetExitCodeProcess(self.handle.0, &mut code) })?;
        ensure!(code == 259, "Game process exited");
        Ok(())
    }
}
pub fn find_game(budget: Arc<ReadBudget>) -> Result<Process> {
    budget.check()?;
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    ensure!(raw != INVALID_HANDLE_VALUE, "Cannot enumerate processes");
    let snapshot = OwnedHandle(raw);
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ids = vec![];
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while ok != 0 {
        budget.check()?;
        let n = entry.szExeFile.iter().position(|x| *x == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..n]).eq_ignore_ascii_case(GAME) {
            ids.push(entry.th32ProcessID);
        }
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    if ids.is_empty() {
        bail!("Game is not running")
    }
    ensure!(ids.len() == 1, "Multiple game processes; refusing ambiguous session");
    open(ids[0], budget).context("Opening installed StarRupture")
}
#[async_trait::async_trait(?Send)]
impl Mem for Process {
    async fn read_buf(&self, address: u64, buf: &mut [u8]) -> Result<()> {
        self.budget.check()?;
        ensure!(
            (0x10000..0x7fffffffffff).contains(&address) && buf.len() <= 16 * 1024 * 1024,
            "Invalid bounded memory read"
        );
        address.checked_add(buf.len() as u64).context("Address overflow")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_or_expired_budget_rejects_reads_before_os_access() {
        let stop = Arc::new(StopSignal::default());
        let b = ReadBudget::new(stop.clone());
        assert!(b.check().is_ok());
        stop.request();
        assert!(b.check().is_err());
        let stop = Arc::new(StopSignal::default());
        let b = ReadBudget::new(stop);
        *b.deadline.lock().unwrap() = Instant::now() - Duration::from_secs(1);
        assert!(b.check().is_err());
        b.renew(1000);
        assert!(b.check().is_ok());
    }
}
