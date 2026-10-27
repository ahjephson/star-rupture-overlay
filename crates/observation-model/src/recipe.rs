//! Recipe identity, validated research arithmetic and generation-safe requests.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    pub name: String,
    pub image: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub item_id: String,
    pub requirements: BTreeMap<String, u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalogue {
    pub game_hash: String,
    pub recipes: BTreeMap<String, Recipe>,
    pub items: BTreeMap<String, Item>,
    pub tick: String,
    pub frame: String,
    pub background: String,
    pub font: String,
}
impl Catalogue {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.game_hash != crate::overlay_stream::GAME_HASH
            || self.recipes.is_empty()
            || self.recipes.len() > 512
            || self.items.len() > 1024
        {
            return Err("Unsupported recipe reference pack");
        }
        for (id, r) in &self.recipes {
            if !id.starts_with("/Game/")
                || r.name.is_empty()
                || r.name.len() > 240
                || !self.items.contains_key(&r.item_id)
                || r.requirements.is_empty()
                || r.requirements.len() > 20
                || r.requirements.iter().any(|(id, n)| *n == 0 || !self.items.contains_key(id))
            {
                return Err("Invalid recipe reference");
            }
        }
        for i in self.items.values() {
            if i.name.is_empty() || i.name.len() > 240 || !safe_file(&i.image) {
                return Err("Invalid recipe icon reference");
            }
        }
        if [&self.tick, &self.frame, &self.background, &self.font].iter().any(|s| !safe_file(s)) {
            return Err("Invalid recipe style reference");
        }
        Ok(())
    }
}
pub fn safe_file(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 180
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        && !s.contains("..")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ingredient {
    pub item_id: String,
    pub submitted: u32,
    pub required: u32,
    pub remaining: u32,
    pub bag_count: Option<u32>,
}
pub fn progress(
    required: &BTreeMap<String, u32>,
    submitted: &[(String, u32)],
) -> Result<Vec<Ingredient>, &'static str> {
    let mut seen = BTreeSet::new();
    if submitted.len() != required.len()
        || submitted.iter().any(|(id, _)| !required.contains_key(id) || !seen.insert(id))
    {
        return Err("Research ingredient set changed");
    }
    Ok(submitted
        .iter()
        .map(|(id, n)| Ingredient {
            item_id: id.clone(),
            submitted: *n,
            required: required[id],
            remaining: required[id].saturating_sub(*n),
            bag_count: None,
        })
        .collect())
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub recipe_id: String,
    pub ingredients: Vec<Ingredient>,
    pub observed_at_ms: u64,
    pub generation: u64,
    pub process: u32,
}
#[derive(Clone, Debug, Default)]
pub struct View {
    pub snapshot: Option<Snapshot>,
    pub message: String,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub generation: u64,
    pub active: bool,
    pub cursor: Option<[i32; 2]>,
}
#[derive(Default)]
pub struct Control {
    request: Mutex<Request>,
    view: Mutex<View>,
}
impl Control {
    pub fn request(&self) -> Request {
        *self.request.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn toggle(&self) {
        self.toggle_at(None);
    }
    pub fn toggle_at(&self, cursor: Option<[i32; 2]>) {
        let mut r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        r.generation = r.generation.wrapping_add(1);
        r.active = true;
        r.cursor = cursor;
        // Keep the current panel intact until a valid replacement is ready.
    }
    pub fn clear(&self) {
        let mut r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        r.generation = r.generation.wrapping_add(1);
        r.active = false;
        r.cursor = None;
        *self.view.lock().unwrap_or_else(|e| e.into_inner()) = View {
            generation: r.generation,
            ..View::default()
        };
    }
    pub fn publish(&self, request: Request, snapshot: Option<Snapshot>, message: String) {
        let mut r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        if *r != request {
            return;
        }
        // A fully submitted update closes the pin. A successful reader absence
        // arrives as None; unavailable observations use the separate error path.
        let snapshot = snapshot
            .filter(|s| s.ingredients.is_empty() || s.ingredients.iter().any(|i| i.remaining > 0));
        if snapshot.is_none() {
            r.active = false
        }
        *self.view.lock().unwrap_or_else(|e| e.into_inner()) = View {
            snapshot,
            message,
            generation: request.generation,
        };
    }
    /// Clear only the request whose outside-station observation was validated.
    pub fn clear_requested(&self, request: Request) {
        let mut r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        if *r != request {
            return;
        }
        r.active = false;
        r.cursor = None;
        *self.view.lock().unwrap_or_else(|e| e.into_inner()) = View {
            generation: r.generation,
            ..View::default()
        };
    }
    /// Commit a capture only after live requirements are available. Invalid or
    /// completed targets leave the selection untouched; the same valid recipe
    /// clears it, and a different valid recipe replaces it atomically.
    pub fn captured(&self, request: Request, snapshot: Option<Snapshot>) {
        let mut r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        if *r != request {
            return;
        }
        let mut view = self.view.lock().unwrap_or_else(|e| e.into_inner());
        r.cursor = None;
        match snapshot {
            Some(mut next) if next.ingredients.iter().any(|i| i.remaining > 0) => {
                if view.snapshot.as_ref().is_some_and(|old| old.recipe_id == next.recipe_id) {
                    r.active = false;
                    *view = View {
                        generation: r.generation,
                        ..View::default()
                    };
                } else {
                    r.active = true;
                    next.generation = r.generation;
                    *view = View {
                        snapshot: Some(next),
                        message: String::new(),
                        generation: r.generation,
                    };
                }
            }
            _ => {
                r.active = view.snapshot.is_some();
                view.generation = r.generation;
                if let Some(old) = &mut view.snapshot {
                    old.generation = r.generation;
                }
            }
        }
    }
    pub fn unavailable(&self, request: Request, message: String) {
        let r = self.request.lock().unwrap_or_else(|e| e.into_inner());
        if *r == request {
            self.view.lock().unwrap_or_else(|e| e.into_inner()).message = message
        }
    }
    pub fn view(&self) -> View {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(id: &str, remaining: u32) -> Snapshot {
        Snapshot {
            recipe_id: id.into(),
            ingredients: vec![Ingredient {
                item_id: "item".into(),
                submitted: 400 - remaining,
                required: 400,
                remaining,
                bag_count: Some(300),
            }],
            observed_at_ms: 1000,
            generation: 0,
            process: 1,
        }
    }
    fn capture(c: &Control, next: Option<Snapshot>) {
        c.toggle_at(Some([20, 30]));
        c.captured(c.request(), next);
    }
    #[test]
    fn invalid_complete_switch_same_and_pending_capture_behavior() {
        let c = Control::default();
        c.toggle_at(Some([20, 30]));
        assert!(c.view().snapshot.is_none());
        assert!(c.view().message.is_empty());
        c.captured(c.request(), None);
        assert!(c.view().snapshot.is_none());
        capture(&c, Some(sample("A", 400)));
        capture(&c, None);
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        capture(&c, Some(sample("B", 0)));
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        capture(&c, Some(sample("A", 0)));
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        c.toggle_at(Some([20, 30]));
        let old = c.request();
        c.toggle_at(Some([40, 50]));
        c.captured(old, Some(sample("stale", 300)));
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        c.captured(c.request(), Some(sample("B", 100)));
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "B");
        capture(&c, Some(sample("B", 100)));
        assert!(c.view().snapshot.is_none());
        assert!(!c.request().active);
    }
    #[test]
    fn outside_station_clears_only_the_current_request() {
        let c = Control::default();
        c.toggle_at(Some([20, 30]));
        c.clear_requested(c.request());
        assert!(!c.request().active);
        assert!(c.view().snapshot.is_none());
        capture(&c, Some(sample("A", 100)));
        c.toggle_at(Some([20, 30]));
        let old = c.request();
        c.toggle_at(Some([30, 40]));
        c.clear_requested(old);
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        c.clear_requested(c.request());
        c.captured(old, Some(sample("B", 100)));
        assert!(c.view().snapshot.is_none());
        assert!(!c.request().active);
    }
    #[test]
    fn partial_missing_duplicate_and_excess() {
        let r = BTreeMap::from([("a".into(), 300), ("b".into(), 600)]);
        let p = progress(&r, &[("a".into(), 100), ("b".into(), 0)]).unwrap();
        assert_eq!(p[0].remaining, 200);
        assert!(progress(&r, &[("a".into(), 100)]).is_err());
        assert!(progress(&r, &[("a".into(), 100), ("a".into(), 0)]).is_err());
        assert_eq!(progress(&r, &[("a".into(), 400), ("b".into(), 600)]).unwrap()[0].remaining, 0);
    }
    #[test]
    fn completed_updates_close_the_pin_but_errors_and_stale_updates_do_not() {
        let c = Control::default();
        capture(&c, Some(sample("A", 100)));
        let old = c.request();
        c.unavailable(old, "temporary read error".into());
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "A");
        c.publish(old, Some(sample("A", 0)), String::new());
        assert!(c.view().snapshot.is_none());
        assert!(!c.request().active);
        capture(&c, Some(sample("A", 100)));
        let stale = c.request();
        capture(&c, Some(sample("B", 100)));
        c.publish(stale, Some(sample("A", 0)), String::new());
        c.publish(stale, None, String::new());
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "B");
        c.publish(c.request(), None, String::new());
        assert!(c.view().snapshot.is_none());
        assert!(!c.request().active);
    }
    #[test]
    fn clearing_blocks_delayed_capture() {
        let c = Control::default();
        c.toggle();
        let old = c.request();
        c.clear();
        c.publish(old, None, "old result".into());
        assert!(c.view().message.is_empty());
        assert!(!c.request().active);
        c.toggle();
        assert!(c.request().active);
    }
    #[test]
    fn rejects_unsafe_paths() {
        for p in ["../icon", "C:/icon", "a/b", "a\\b", ""] {
            assert!(!safe_file(p))
        }
        assert!(safe_file("icon-123.bgra"));
    }
}
