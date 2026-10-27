//! A cancellable tree of owned setup processes; never targets the game process.
use anyhow::{Context, Result, bail, ensure};
use starrupture_observation_model::shutdown::StopSignal;
use std::{
    mem::size_of,
    path::Path,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{JobObjects::*, Threading::*},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
}
fn quoted(value: &str) -> String {
    let mut text = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            text.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            text.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        text.push(c);
    }
    text.push_str(&"\\".repeat(slashes * 2));
    text.push('"');
    text
}
pub(super) fn run_executable(
    executable: &Path,
    request: &Path,
    error: &Path,
    stop: &StopSignal,
    enabled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    let line = format!(
        "{} --prepare-recipe-assets {}",
        quoted(&executable.to_string_lossy()),
        quoted(&request.to_string_lossy())
    );
    run_process(executable, &line, error, stop, enabled)
}
fn run_process(
    executable: &Path,
    line: &str,
    error: &Path,
    stop: &StopSignal,
    enabled: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    ensure!(line.len() < 30000, "Setup command length bound");
    let mut line = crate::desktop::wide(line);
    unsafe {
        let job = Handle(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
        ensure!(!job.0.is_null(), "Could not create recipe setup job");
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        ensure!(
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32
            ) != 0,
            "Could not guard setup process lifetime"
        );
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            ..std::mem::zeroed()
        };
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        ensure!(
            CreateProcessW(
                crate::desktop::wide(executable).as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_NO_WINDOW | CREATE_SUSPENDED,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut info
            ) != 0,
            "Could not start bundled asset preparation: {}",
            std::io::Error::last_os_error()
        );
        let process = Handle(info.hProcess);
        let thread = Handle(info.hThread);
        if AssignProcessToJobObject(job.0, process.0) == 0 {
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, 5000);
            bail!("Could not guard recipe setup child processes");
        }
        ensure!(ResumeThread(thread.0) != u32::MAX, "Could not resume recipe setup");
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if stop.is_requested()
                || !enabled.load(std::sync::atomic::Ordering::Relaxed)
                || Instant::now() >= deadline
            {
                TerminateJobObject(job.0, 1);
                WaitForSingleObject(process.0, 5000);
                bail!("Recipe setup cancelled or timed out");
            }
            match WaitForSingleObject(process.0, 100) {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => (),
                _ => bail!("Recipe setup process wait failed"),
            }
        }
        let mut exit = 1;
        ensure!(
            GetExitCodeProcess(process.0, &mut exit) != 0,
            "Recipe setup exit status unavailable"
        );
        if exit != 0 {
            let reason = std::fs::read_to_string(error).unwrap_or_default();
            bail!("Recipe preparation failed: {}", reason.chars().take(300).collect::<String>());
        }
    }
    Ok(())
}

pub fn self_test(directory: &Path) -> Result<()> {
    ensure!(directory.is_absolute(), "Absolute cancellation fixture directory required");
    std::fs::create_dir_all(directory)?;
    let marker = directory.join("owned-child-pid.txt");
    ensure!(!marker.exists(), "Cancellation fixture must be fresh");
    let stop = std::sync::Arc::new(StopSignal::default());
    let cancel = stop.clone();
    let wait_marker = marker.clone();
    let waiter = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(text) = std::fs::read_to_string(&wait_marker)
                && let Ok(pid) = text.parse::<u32>()
            {
                cancel.request();
                return Some(pid);
            }
            if Instant::now() >= deadline {
                cancel.request();
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let started = Instant::now();
    let exe = std::env::current_exe()?;
    let line = format!(
        "{} --self-test-asset-wait {}",
        quoted(&exe.to_string_lossy()),
        quoted(&marker.to_string_lossy())
    );
    let result = run_process(
        &exe,
        &line,
        &directory.join("error.txt"),
        &stop,
        &std::sync::atomic::AtomicBool::new(true),
    );
    let pid = waiter
        .join()
        .map_err(|_| anyhow::anyhow!("Cancellation fixture thread failed"))?
        .context("Setup fixture did not start its child")?;
    ensure!(result.is_err(), "Cancelled setup unexpectedly succeeded");
    unsafe {
        let process = Handle(OpenProcess(PROCESS_SYNCHRONIZE, 0, pid));
        if !process.0.is_null() {
            ensure!(
                WaitForSingleObject(process.0, 3000) == WAIT_OBJECT_0,
                "Setup left a child process running"
            );
        }
    }
    std::fs::write(
        directory.join("asset-cancellation-validation.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"ownedProcessesOnly":true,"cancelled":true,"childStopped":true,"elapsedMillis":started.elapsed().as_millis()}),
        )?,
    )?;
    Ok(())
}

pub fn wait_fixture(marker: &Path, child: bool) -> Result<()> {
    use std::{os::windows::process::CommandExt, process::Command};
    ensure!(marker.is_absolute(), "Absolute cancellation marker required");
    if child {
        std::fs::write(marker, std::process::id().to_string())?;
    } else {
        let mut process = Command::new(std::env::current_exe()?)
            .arg("--self-test-asset-wait-child")
            .arg(marker)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        let _ = process.wait()?;
    }
    std::thread::sleep(Duration::from_secs(120));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments_preserve_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quoted("a b"), "\"a b\"");
        assert_eq!(quoted("a\"b"), "\"a\\\"b\"");
        assert_eq!(quoted("tail\\"), "\"tail\\\\\"");
    }
}
