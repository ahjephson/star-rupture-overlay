//! Per-user preferences are loaded once and saved only on an explicit action.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::os::windows::ffi::OsStrExt;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameExit {
    #[default]
    KeepRunning,
    Ask,
    Exit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub game_exit: GameExit,
}
pub fn directory() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA").context("Local application-data directory unavailable")?,
    )
    .join("StarRuptureWaveOverlay"))
}
pub fn load(path: &Path) -> Result<Preferences> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "Preferences file exceeds its size limit");
    serde_json::from_slice(&bytes).context("Could not read preferences")
}
pub fn save(path: &Path, preferences: Preferences) -> Result<()> {
    let parent = path.parent().context("Preferences directory unavailable")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec_pretty(&preferences)?)?;
        file.sync_all()?;
        drop(file);
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain([0]).collect();
        let to: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        ensure!(
            unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } != 0,
            "Could not replace preferences: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_round_trip_and_failures_preserve_previous_file() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local/preferences-tests")
            .join(format!("{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("preferences.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        for game_exit in [GameExit::Ask, GameExit::Exit, GameExit::KeepRunning] {
            let settings = Preferences {
                game_exit,
            };
            save(&path, settings).unwrap();
            assert_eq!(load(&path).unwrap(), settings);
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, b"occupied").unwrap();
        assert!(
            save(
                &path,
                Preferences {
                    game_exit: GameExit::Exit
                }
            )
            .is_err()
        );
        assert_eq!(load(&path).unwrap(), Preferences::default());
        fs::remove_file(temporary).unwrap();
        fs::write(&path, b"{\"gameExit\":\"invalid\"}").unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, vec![b' '; 4097]).unwrap();
        assert!(load(&path).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
