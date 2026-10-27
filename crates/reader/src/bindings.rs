//! Runtime player offsets resolved from a validated cache record.
use crate::{Cache, Identity, key, validate_offsets};
use anyhow::{Context, Result, ensure};
use std::ops::Deref;
pub(crate) struct RuntimeCache {
    pub record: Cache,
    pub fields: PlayerFields,
}
#[derive(Clone, Copy)]
pub(crate) struct PlayerFields {
    pub player: u64,
    pub acknowledged_pawn: u64,
    pub pawn: u64,
    pub character: u64,
    pub player_controller: u64,
    pub root_component: u64,
    pub attach_parent: u64,
    pub relative_location: u64,
}
impl RuntimeCache {
    pub fn new(record: Cache) -> Result<Self> {
        validate_offsets(&record.offsets)?;
        let offset = |class, field| {
            record
                .offsets
                .get(&key(class, field))
                .copied()
                .map(|v| v as u64)
                .context("Missing reflected field")
        };
        let fields = PlayerFields {
            player: offset("/Script/Engine.PlayerController", "Player")?,
            acknowledged_pawn: offset("/Script/Engine.PlayerController", "AcknowledgedPawn")?,
            pawn: offset("/Script/Engine.Controller", "Pawn")?,
            character: offset("/Script/Engine.Controller", "Character")?,
            player_controller: offset("/Script/Engine.Player", "PlayerController")?,
            root_component: offset("/Script/Engine.Actor", "RootComponent")?,
            attach_parent: offset("/Script/Engine.SceneComponent", "AttachParent")?,
            relative_location: offset("/Script/Engine.SceneComponent", "RelativeLocation")?,
        };
        Ok(Self {
            record,
            fields,
        })
    }
    pub fn matches(&self, identity: &Identity) -> bool {
        self.record.version == 1
            && self.record.identity == *identity
            && self.fields.validate().is_ok()
    }
}
impl PlayerFields {
    fn validate(self) -> Result<()> {
        ensure!(
            [
                self.player,
                self.acknowledged_pawn,
                self.pawn,
                self.character,
                self.player_controller,
                self.root_component,
                self.attach_parent,
                self.relative_location
            ]
            .into_iter()
            .all(|v| (8..=262144).contains(&v)),
            "Invalid reflected offset"
        );
        Ok(())
    }
}
impl Deref for RuntimeCache {
    type Target = Cache;
    fn deref(&self) -> &Self::Target {
        &self.record
    }
}
