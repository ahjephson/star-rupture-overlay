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
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Preferences {
    pub countdown_format: starrupture_observation_model::wave::CountdownFormat,
    pub game_exit: GameExit,
    pub recipe_tracker: RecipeSettings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct RecipeSettings {
    pub enabled: bool,
    pub key: u16,
    pub modifiers: u16,
    pub position: u8,
    pub scale: u16,
}
impl Default for RecipeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            key: 0x58,
            modifiers: 1,
            position: 1,
            scale: 100,
        }
    }
}
impl RecipeSettings {
    pub fn shortcut_label(self) -> String {
        let mut parts = Vec::new();
        if self.modifiers & 2 != 0 {
            parts.push("Ctrl".to_owned())
        }
        if self.modifiers & 1 != 0 {
            parts.push("Alt".to_owned())
        }
        if self.modifiers & 4 != 0 {
            parts.push("Shift".to_owned())
        }
        parts.push(if self.key >= 0x70 {
            format!("F{}", self.key - 0x70 + 1)
        } else {
            char::from_u32(self.key as u32).unwrap_or('?').to_string()
        });
        parts.join("+")
    }
    pub fn validate(self) -> Result<()> {
        ensure!(
            (0x30..=0x39).contains(&self.key)
                || (0x41..=0x5a).contains(&self.key)
                || (0x70..=0x7a).contains(&self.key),
            "Choose a letter, number or F1–F11 shortcut."
        );
        ensure!(
            self.modifiers & !7 == 0 && self.modifiers & 3 != 0,
            "Include Alt or Ctrl in the shortcut."
        );
        ensure!(
            self.position <= 2 && [75, 100, 125, 150].contains(&self.scale),
            "Invalid recipe panel position or scale."
        );
        Ok(())
    }
}
pub fn directory() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA").context("Local application-data directory unavailable")?,
    )
    .join("StarRuptureWaveOverlay"))
}
pub fn parse_shortcut(value: &str) -> Result<(u16, u16)> {
    ensure!(value.len() <= 48, "Shortcut is too long. Try Alt+X.");
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    ensure!(parts.len() >= 2 && parts.len() <= 4, "Enter a shortcut such as Alt+X or Ctrl+Alt+R.");
    let mut modifiers = 0u16;
    for part in &parts[..parts.len() - 1] {
        let flag = match part.to_ascii_lowercase().as_str() {
            "alt" => 1,
            "ctrl" | "control" => 2,
            "shift" => 4,
            _ => anyhow::bail!("Use Alt, Ctrl or Shift in the shortcut."),
        };
        ensure!(modifiers & flag == 0, "Shortcut modifiers must appear once.");
        modifiers |= flag;
    }
    let last = parts.last().unwrap().to_ascii_uppercase();
    let key = if last.len() == 1 && last.as_bytes()[0].is_ascii_alphanumeric() {
        last.as_bytes()[0] as u16
    } else if let Some(number) = last.strip_prefix('F') {
        let n = number.parse::<u16>().context("Choose a letter, number or F1–F11.")?;
        ensure!((1..=11).contains(&n), "Choose F1–F11; F12 is reserved by Windows.");
        0x70 + n - 1
    } else {
        anyhow::bail!("Choose a letter, number or F1–F11.")
    };
    RecipeSettings {
        key,
        modifiers,
        ..RecipeSettings::default()
    }
    .validate()?;
    Ok((key, modifiers))
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
    let preferences: Preferences =
        serde_json::from_slice(&bytes).context("Could not read preferences")?;
    preferences.recipe_tracker.validate()?;
    Ok(preferences)
}
pub fn save(path: &Path, preferences: Preferences) -> Result<()> {
    preferences.recipe_tracker.validate()?;
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
    fn old_preferences_and_shortcut_validation() {
        assert_eq!(parse_shortcut("Alt+X").unwrap(), (0x58, 1));
        assert_eq!(parse_shortcut("Ctrl+Alt+F9").unwrap(), (0x78, 3));
        for bad in ["F12", "Alt+F12", "Alt+Alt+X", "Shift+X", "Win+X"] {
            assert!(parse_shortcut(bad).is_err());
        }
        let old: Preferences = serde_json::from_str(r#"{"gameExit":"ask"}"#).unwrap();
        assert_eq!(old.recipe_tracker, RecipeSettings::default());
        assert_eq!(
            old.countdown_format,
            starrupture_observation_model::wave::CountdownFormat::Hours
        );
        assert!(
            RecipeSettings {
                key: 0x7b,
                ..RecipeSettings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            RecipeSettings {
                modifiers: 0,
                ..RecipeSettings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            RecipeSettings {
                key: 0x4a,
                modifiers: 3,
                scale: 125,
                position: 2,
                enabled: true
            }
            .validate()
            .is_ok()
        );
    }
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
                ..Preferences::default()
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
                    game_exit: GameExit::Exit,
                    ..Preferences::default()
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
