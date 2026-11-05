//! Narrow OS boundary: bounded reads through query/read-only handles.
//!
//! Buffers remain owned across each synchronous Win32 call. Handles close via
//! RAII on the reader thread; no process write, code execution or privilege APIs.
use crate::{Identity, Mem};
use anyhow::{Context, Result, ensure};
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
fn supported_filename(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(GAME))
}

#[derive(Debug)]
pub(crate) struct GameNotRunning;
impl std::fmt::Display for GameNotRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Game is not running")
    }
}
impl std::error::Error for GameNotRunning {}
struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: This guard owns the non-shared Win32 handle returned by its creation/open API and
        // closes it once without transferring Rust memory.
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
    // SAFETY: Only query/read or synchronization rights are requested; Win32 validates the scalar
    // PID, and a successful handle is retained by an RAII owner.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    ensure!(!raw.is_null(), "Cannot open game read-only: {}", std::io::Error::last_os_error());
    // Process handles stay on the reader thread; only cancellation is shared.
    let handle = Rc::new(OwnedHandle(raw));
    let mut chars = vec![0u16; 32768];
    let mut count = chars.len() as u32;
    // SAFETY: The retained process handle is query-only; the UTF-16 output slice is writable for
    // count elements, and its length is used only after a successful call.
    checked(unsafe { QueryFullProcessImageNameW(raw, 0, chars.as_mut_ptr(), &mut count) })?;
    let executable = String::from_utf16(&chars[..count as usize])?;
    ensure!(supported_filename(&executable), "Unexpected game executable filename");
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut created = unsafe { zeroed() };
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut exited = unsafe { zeroed() };
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut kernel = unsafe { zeroed() };
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut user = unsafe { zeroed() };
    // SAFETY: The process handle is retained and all four FILETIME outputs are initialized writable
    // locals with no aliases during the synchronous query.
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
    pub(crate) fn main_module(&self) -> Result<(u64, usize)> {
        self.ensure_alive()?;
        // SAFETY: Snapshot flags and process IDs are scalar inputs; the returned handle is checked
        // before use and closed by its owning guard.
        let raw = unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.identity.pid)
        };
        ensure!(raw != INVALID_HANDLE_VALUE, "Cannot enumerate game module");
        let snapshot = OwnedHandle(raw);
        // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-
        // zero value is valid before the API fills it.
        let mut module: MODULEENTRY32W = unsafe { zeroed() };
        module.dwSize = size_of::<MODULEENTRY32W>() as u32;
        // SAFETY: The snapshot handle is retained, and the initialized entry has dwSize set to its
        // C-structure size; the API writes only that local entry.
        checked(unsafe { Module32FirstW(snapshot.0, &mut module) })?;
        let n = module.szExePath.iter().position(|v| *v == 0).unwrap_or(module.szExePath.len());
        ensure!(
            String::from_utf16(&module.szExePath[..n])?
                .eq_ignore_ascii_case(&self.identity.executable),
            "Game module identity changed"
        );
        self.ensure_alive()?;
        Ok((module.modBaseAddr as u64, module.modBaseSize as usize))
    }
    pub(crate) fn has_exited(&self) -> Result<bool> {
        self.budget.check()?;
        let mut code = 0;
        // SAFETY: The process handle remains owned for this query, and the exit-status output is an
        // initialized writable local.
        checked(unsafe { GetExitCodeProcess(self.handle.0, &mut code) })?;
        Ok(code != 259)
    }
    pub fn ensure_alive(&self) -> Result<()> {
        self.budget.check()?;
        let mut code = 0;
        // SAFETY: The process handle remains owned for this query, and the exit-status output is an
        // initialized writable local.
        checked(unsafe { GetExitCodeProcess(self.handle.0, &mut code) })?;
        ensure!(code == 259, "Game process exited");
        Ok(())
    }
}
pub fn find_game(budget: Arc<ReadBudget>) -> Result<Process> {
    budget.check()?;
    // SAFETY: Snapshot flags and process IDs are scalar inputs; the returned handle is checked
    // before use and closed by its owning guard.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    ensure!(raw != INVALID_HANDLE_VALUE, "Cannot enumerate processes");
    let snapshot = OwnedHandle(raw);
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ids = vec![];
    // SAFETY: The snapshot handle is retained, and the initialized entry has dwSize set to its
    // C-structure size; the API writes only that local entry.
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while ok != 0 {
        budget.check()?;
        let n = entry.szExeFile.iter().position(|x| *x == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..n]).eq_ignore_ascii_case(GAME) {
            ids.push(entry.th32ProcessID);
        }
        // SAFETY: The snapshot handle is retained, and the initialized entry has dwSize set to its
        // C-structure size; the API writes only that local entry.
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    if ids.is_empty() {
        return Err(GameNotRunning.into());
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
        // SAFETY: The retained handle has query/read access only; the checked remote range is
        // copied into the exclusively borrowed output slice with its exact capacity, and the byte-
        // count output is writable. Windows reports an unreadable remote range as failure.
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
    // SAFETY: The current-process/token handle is retained; output structs and returned-size
    // storage are initialized with matching byte lengths. The token is closed by its owning guard.
    checked(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) })?;
    let h = OwnedHandle(raw);
    // SAFETY: This Win32 output structure contains only integer and raw-pointer fields; an all-zero
    // value is valid before the API fills it.
    let mut e: TOKEN_ELEVATION = unsafe { zeroed() };
    let mut returned = 0;
    // SAFETY: The current-process/token handle is retained; output structs and returned-size
    // storage are initialized with matching byte lengths. The token is closed by its owning guard.
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

/// One unnamed high-resolution timer for short navigation deadlines only. Long
/// waits keep the existing interruptible stop signal; no system timer period changes.
pub(crate) struct ShortWait(Option<OwnedHandle>);
impl ShortWait {
    pub(crate) fn new() -> Self {
        // SAFETY: Null security/name create an unnamed, non-inheritable owned timer;
        // it has no callback and the returned handle is closed by OwnedHandle.
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        Self((!handle.is_null()).then_some(OwnedHandle(handle)))
    }
    pub(crate) fn until(&self, stop: &StopSignal, deadline: Instant) {
        if stop.is_requested() {
            return;
        }
        let delay = deadline.saturating_duration_since(Instant::now());
        if delay.is_zero() {
            return;
        }
        if delay <= Duration::from_millis(50)
            && let Some(timer) = &self.0
        {
            // delay is bounded above, so nanoseconds/100 fits i64. A strictly
            // negative due time is relative; no APC, power request or game handle.
            let due = -(i64::try_from(delay.as_nanos() / 100).unwrap_or(500000).max(1));
            // SAFETY: The worker owns this timer and the scalar due-time remains
            // live for the synchronous setter. The native wait is capped at 50 ms,
            // bounding cancellation latency even if the timer cannot be signalled.
            let completed = unsafe {
                SetWaitableTimer(timer.0, &due, 0, None, std::ptr::null(), 0) != 0
                    && WaitForSingleObject(timer.0, 50) == WAIT_OBJECT_0
            };
            if completed {
                return;
            }
        }
        stop.wait_until(deadline);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn short_wait_obeys_deadline_and_already_requested_shutdown() {
        let wait = super::ShortWait::new();
        let stop = super::StopSignal::default();
        let deadline = super::Instant::now() + super::Duration::from_millis(2);
        wait.until(&stop, deadline);
        assert!(super::Instant::now() >= deadline);
        stop.request();
        let start = super::Instant::now();
        wait.until(&stop, start + super::Duration::from_secs(1));
        assert!(start.elapsed() < super::Duration::from_millis(100));
    }
    use super::*;
    #[test]
    fn executable_filename_is_independent_of_install_directory() {
        for path in [
            r"C:\Program Files (x86)\Steam\steamapps\common\StarRupture\StarRupture\Binaries\Win64\StarRuptureGameSteam-Win64-Shipping.exe",
            r"D:\Games\SteamLibrary\StarRuptureGameSteam-Win64-Shipping.exe",
            r"E:\Other program files\STARRUPTUREGAMESTEAM-WIN64-SHIPPING.EXE",
        ] {
            assert!(supported_filename(path));
        }
        assert!(!supported_filename(r"D:\Games\Other.exe"));
        assert!(!supported_filename(r"D:\Games\StarRuptureGameSteam-Win64-Shipping.exe.bak"));
    }
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
