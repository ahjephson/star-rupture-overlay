//! Build/mode-scoped preparation inputs. No process addresses or player progress are stored.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeContext {
    pub version: u32,
    pub game_hash: String,
    pub mode_class: String,
    pub state_class: String,
    pub keeper_class: String,
    pub buildings: Vec<BuildingRoot>,
    pub weapon_classes: Vec<String>,
    pub core_asset: String,
    pub defaults: Defaults,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildingRoot {
    pub id: String,
    pub category: u8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Defaults {
    pub add_platform: bool,
    pub add_pillar: bool,
    pub recipe_seconds: f32,
    pub crafting_loop_seconds: f32,
    pub overwrite_recipe_seconds: f32,
    pub use_crafting_processor: bool,
}
pub fn asset_id(value: &str) -> bool {
    value.len() <= 512
        && (value.starts_with("/Game/Chimera/") || value.starts_with("/Chimera"))
        && !value.contains("..")
        && !value.ends_with('/')
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-".contains(&b))
}
pub fn class_path(value: &str) -> bool {
    value.split_once('.').is_some_and(|(package, name)| {
        asset_id(package)
            && !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
impl NativeContext {
    pub fn validate(&self) -> Result<(), String> {
        let d = &self.defaults;
        if self.version != 1
            || !crate::overlay_stream::valid_executable_hash(&self.game_hash)
            || ![&self.mode_class, &self.state_class, &self.keeper_class]
                .iter()
                .all(|s| class_path(s))
            || !asset_id(&self.core_asset)
            || self.buildings.is_empty()
            || self.buildings.len() > 1024
            || self.weapon_classes.is_empty()
            || self.weapon_classes.len() > 128
            || !d.recipe_seconds.is_finite()
            || d.recipe_seconds < 0.0
            || d.recipe_seconds > 86400.0
            || !d.crafting_loop_seconds.is_finite()
            || d.crafting_loop_seconds < 0.0
            || !d.overwrite_recipe_seconds.is_finite()
            || d.overwrite_recipe_seconds < -1.0
        {
            return Err("invalid catalogue preparation context".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        if !self.buildings.iter().all(|b| asset_id(&b.id) && b.category <= 11 && ids.insert(&b.id))
        {
            return Err("invalid or duplicate authored building".into());
        }
        let mut classes = std::collections::BTreeSet::new();
        if !self.weapon_classes.iter().all(|c| class_path(c) && classes.insert(c)) {
            return Err("invalid or duplicate registered weapon class".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_ids_cannot_be_filesystem_paths_or_subobjects() {
        assert!(asset_id("/Game/Chimera/Items/I_Material"));
        assert!(asset_id("/ChimeraWeapon/Data/I_Weapon"));
        for s in [
            "../file",
            "/Game/Chimera/../x",
            "/Game/Chimera/X.X_C",
            "C:/game/x",
            "/Game/Chimera/X:Child",
        ] {
            assert!(!asset_id(s));
        }
        assert!(class_path("/ChimeraWeapon/Data/I_Weapon.I_Weapon_C"));
    }
}
