use crate::AppError;
use serde::Deserialize;
use std::{fs::File,io::Read,path::{Path,PathBuf}};
#[derive(Debug,Clone,Default,Deserialize)]
#[serde(default,deny_unknown_fields,rename_all="camelCase")]
pub struct Config {
    pub cache_directory:Option<PathBuf>,
    pub live:Option<Live>,
    // Accepted only as false for compatibility with the initial overlay configuration.
    pub auto_discover:bool,
}
#[derive(Debug,Clone,Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Live {pub enabled:bool,pub helper_file:PathBuf,pub helper_sha256:String}
impl Config {
    pub fn load(path:Option<&Path>)->Result<Self,AppError> {
        let Some(path)=path else {return Ok(Self::default())};
        if !path.exists(){return Err(AppError::new("CONFIG_NOT_FOUND","Explicit configuration does not exist"));}
        let invalid=|e|AppError::new("CONFIG_INVALID",e);
        let file=File::open(path).map_err(|e|invalid(e.to_string()))?;
        let mut bytes=Vec::new();file.take(65537).read_to_end(&mut bytes).map_err(|e|invalid(e.to_string()))?;
        if bytes.len()>65536 {return Err(invalid("Configuration exceeds read bound".into()));}
        let mut value:Self=serde_json::from_slice(&bytes).map_err(|e|invalid(e.to_string()))?;
        value.validate()?;
        let base=std::env::current_dir().map_err(|e|invalid(e.to_string()))?.join(path).parent().unwrap().to_path_buf();
        for p in value.cache_directory.iter_mut().chain(value.live.iter_mut().map(|v|&mut v.helper_file)) {if p.is_relative(){*p=base.join(&*p);}}
        Ok(value)
    }
    fn validate(&self)->Result<(),AppError> {
        let paths=self.cache_directory.iter().chain(self.live.iter().map(|v|&v.helper_file));
        if self.auto_discover || !paths.into_iter().all(|p|!p.as_os_str().is_empty()&&!p.to_string_lossy().contains('\0')) {
            return Err(AppError::new("CONFIG_INVALID","Invalid path or unsupported autoDiscover"));
        }
        if self.live.as_ref().is_some_and(|v|v.helper_sha256.len()!=64 || !v.helper_sha256.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))) {
            return Err(AppError::new("CONFIG_INVALID","Helper requires a lowercase SHA256 pin"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn independent_configuration_rejects_mcp_settings_and_unsafe_pins() {
  assert!(serde_json::from_str::<Config>(r#"{"saveControl":{}}"#).is_err());
  let mut c=Config{live:Some(Live{enabled:true,helper_file:"reader.exe".into(),helper_sha256:"a".repeat(64)}),..Default::default()};
  assert!(c.validate().is_ok());c.live.as_mut().unwrap().helper_sha256="A".repeat(64);assert!(c.validate().is_err());
  c.live=None;c.auto_discover=true;assert!(c.validate().is_err());
 }
}
