use crate::AppError;
use serde::Deserialize;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
/// Settings used by the running application.
#[derive(Debug, Clone)]
pub struct Config {
    pub cache_directory: Option<PathBuf>,
    pub live_enabled: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            cache_directory: None,
            live_enabled: true,
        }
    }
}
/// File acceptance is kept separate from the operational settings.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct FileConfig {
    cache_directory: Option<PathBuf>,
    live: Option<CompatibleLiveConfig>,
    auto_discover: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CompatibleLiveConfig {
    enabled: bool,
    helper_file: PathBuf,
    helper_sha256: String,
}
impl FileConfig {
    fn validate(&self) -> Result<(), AppError> {
        let paths = self.cache_directory.iter().chain(self.live.iter().map(|v| &v.helper_file));
        if self.auto_discover
            || !paths
                .into_iter()
                .all(|p| !p.as_os_str().is_empty() && !p.to_string_lossy().contains('\0'))
        {
            return Err(AppError::new(
                "CONFIG_INVALID",
                "Invalid path or unsupported autoDiscover",
            ));
        }
        if self.live.as_ref().is_some_and(|v| {
            v.helper_sha256.len() != 64
                || !v
                    .helper_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(AppError::new("CONFIG_INVALID", "Helper requires a lowercase SHA256 pin"));
        }
        Ok(())
    }
    fn into_runtime(self) -> Result<Config, AppError> {
        self.validate()?;
        Ok(Config {
            cache_directory: self.cache_directory,
            live_enabled: self.live.is_none_or(|v| v.enabled),
        })
    }
}
impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, AppError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Err(AppError::new("CONFIG_NOT_FOUND", "Explicit configuration does not exist"));
        }
        let invalid = |e| AppError::new("CONFIG_INVALID", e);
        let file = File::open(path).map_err(|e| invalid(e.to_string()))?;
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes).map_err(|e| invalid(e.to_string()))?;
        if bytes.len() > 65536 {
            return Err(invalid("Configuration exceeds read bound".into()));
        }
        let mut value = serde_json::from_slice::<FileConfig>(&bytes)
            .map_err(|e| invalid(e.to_string()))?
            .into_runtime()?;
        let absolute = std::env::current_dir().map_err(|e| invalid(e.to_string()))?.join(path);
        let base = absolute
            .parent()
            .ok_or_else(|| invalid("Configuration has no parent directory".into()))?;
        if let Some(p) = &mut value.cache_directory
            && p.is_relative()
        {
            *p = base.join(&*p);
        }
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_acceptance_preserves_existing_keys_and_helper_validation() {
        assert!(serde_json::from_str::<FileConfig>(r#"{"saveControl":{}}"#).is_err());
        let mut c = FileConfig {
            live: Some(CompatibleLiveConfig {
                enabled: true,
                helper_file: "reader.exe".into(),
                helper_sha256: "a".repeat(64),
            }),
            ..FileConfig::default()
        };
        assert!(c.validate().is_ok());
        c.live.as_mut().unwrap().helper_sha256 = "A".repeat(64);
        assert!(c.validate().is_err());
        c.live = None;
        c.auto_discover = true;
        assert!(c.validate().is_err());
    }
    #[test]
    fn compatibility_fields_are_validated_and_only_operational_settings_are_retained() {
        let enabled = FileConfig::default().into_runtime().unwrap();
        assert!(enabled.live_enabled);
        let compatible = FileConfig {
            cache_directory: Some("cache".into()),
            live: Some(CompatibleLiveConfig {
                enabled: false,
                helper_file: "reader.exe".into(),
                helper_sha256: "a".repeat(64),
            }),
            auto_discover: false,
        }
        .into_runtime()
        .unwrap();
        assert!(!compatible.live_enabled);
        assert_eq!(compatible.cache_directory, Some("cache".into()));
        for path in ["", "invalid\0path"] {
            let input = FileConfig {
                cache_directory: Some(path.into()),
                ..FileConfig::default()
            };
            assert!(input.into_runtime().is_err());
        }
    }
}
