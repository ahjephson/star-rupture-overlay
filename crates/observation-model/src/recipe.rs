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
        if !crate::overlay_stream::valid_executable_hash(&self.game_hash)
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
pub const MAX_PINS: usize = 3;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub generation: u64,
    pub active: bool,
    pub cursor: Option<[i32; 2]>,
    /// Only a keyboard request may clear all pins outside the station.
    pub clear_outside: bool,
}
#[derive(Default)]
struct State {
    request: Request,
    pins: Vec<View>,
    notice: String,
}
#[derive(Default)]
pub struct Control {
    state: Mutex<State>,
}
impl State {
    fn advance(&mut self) {
        self.request.generation = self.request.generation.wrapping_add(1);
        self.notice.clear();
        for view in &mut self.pins {
            view.generation = self.request.generation;
            if let Some(snapshot) = &mut view.snapshot {
                snapshot.generation = view.generation;
            }
        }
    }
    fn settle(&mut self) {
        self.request.cursor = None;
        self.request.active = !self.pins.is_empty();
    }
}
impl Control {
    pub fn request(&self) -> Request {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).request
    }
    pub fn toggle(&self) {
        self.toggle_at(None);
    }
    pub fn toggle_at(&self, cursor: Option<[i32; 2]>) {
        self.select(cursor, true);
    }
    pub fn mouse_at(&self, cursor: [i32; 2]) {
        self.select(Some(cursor), false);
    }
    fn select(&self, cursor: Option<[i32; 2]>, clear_outside: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.advance();
        state.request.active = true;
        state.request.cursor = cursor;
        state.request.clear_outside = clear_outside;
    }
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.advance();
        state.pins.clear();
        state.settle();
    }
    pub fn remove(&self, id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.pins.iter().any(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id == id)) {
            return;
        }
        state.advance();
        state.pins.retain(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id != id));
        state.settle();
    }
    pub fn publish_for(
        &self,
        request: Request,
        id: &str,
        snapshot: Option<Snapshot>,
        message: String,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request != request {
            return;
        }
        let Some(index) =
            state.pins.iter().position(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id == id))
        else {
            return;
        };
        if snapshot.as_ref().is_some_and(|s| s.recipe_id != id) {
            return;
        }
        match snapshot
            .filter(|s| s.recipe_id == id && s.ingredients.iter().any(|i| i.remaining > 0))
        {
            Some(mut next) => {
                next.generation = request.generation;
                state.pins[index] = View {
                    snapshot: Some(next),
                    message,
                    generation: request.generation,
                };
            }
            None => {
                state.pins.remove(index);
            }
        }
        state.request.active = !state.pins.is_empty();
    }
    pub fn publish(&self, request: Request, snapshot: Option<Snapshot>, message: String) {
        let id = snapshot
            .as_ref()
            .map(|s| s.recipe_id.clone())
            .or_else(|| self.view().snapshot.map(|s| s.recipe_id));
        if let Some(id) = id {
            self.publish_for(request, &id, snapshot, message);
        }
    }
    pub fn clear_requested(&self, request: Request) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request != request {
            return;
        }
        if request.clear_outside {
            state.pins.clear();
        }
        state.settle();
    }
    /// Only a complete, guarded unfinished capture can add or toggle a pin.
    pub fn captured(&self, request: Request, snapshot: Option<Snapshot>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request != request {
            return;
        }
        if let Some(mut next) = snapshot.filter(|s| s.ingredients.iter().any(|i| i.remaining > 0)) {
            if let Some(index) = state
                .pins
                .iter()
                .position(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id == next.recipe_id))
            {
                state.pins.remove(index);
            } else if state.pins.len() < MAX_PINS {
                next.generation = request.generation;
                state.pins.push(View {
                    snapshot: Some(next),
                    message: String::new(),
                    generation: request.generation,
                });
            } else {
                state.notice = "Three recipes are pinned. Close one before adding another.".into();
            }
        }
        state.settle();
    }
    pub fn unavailable_for(&self, request: Request, id: &str, message: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request == request
            && let Some(view) = state
                .pins
                .iter_mut()
                .find(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id == id))
        {
            view.message = message;
        }
    }
    pub fn unavailable(&self, request: Request, message: String) {
        if let Some(snapshot) = self.view().snapshot {
            self.unavailable_for(request, &snapshot.recipe_id, message);
        }
    }
    pub fn views(&self) -> Vec<View> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).pins.clone()
    }
    pub fn notice(&self) -> String {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).notice.clone()
    }
    pub fn view(&self) -> View {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.pins.first().cloned().unwrap_or_else(|| View {
            generation: state.request.generation,
            ..View::default()
        })
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
    fn captures_three_independently_and_rejects_fourth_without_replacing() {
        let c = Control::default();
        for id in ["A", "B", "C", "D"] {
            capture(&c, Some(sample(id, 100)));
        }
        assert_eq!(
            c.views()
                .iter()
                .map(|v| v.snapshot.as_ref().unwrap().recipe_id.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert!(!c.notice().is_empty());
        capture(&c, Some(sample("B", 100)));
        assert_eq!(c.views().len(), 2);
        assert_eq!(c.views()[1].snapshot.as_ref().unwrap().recipe_id, "C");
        capture(&c, Some(sample("D", 100)));
        assert_eq!(c.views().len(), 3);
    }
    #[test]
    fn invalid_completed_and_outside_mouse_preserve_all_pins_and_settle() {
        let c = Control::default();
        capture(&c, Some(sample("A", 100)));
        capture(&c, Some(sample("B", 100)));
        capture(&c, None);
        capture(&c, Some(sample("A", 0)));
        assert_eq!(c.views().len(), 2);
        assert!(c.request().cursor.is_none());
        c.mouse_at([20, 30]);
        c.clear_requested(c.request());
        assert_eq!(c.views().len(), 2);
        c.toggle_at(Some([20, 30]));
        c.clear_requested(c.request());
        assert!(c.views().is_empty());
        assert!(!c.request().active);
    }
    #[test]
    fn close_invalidates_pending_reads_and_recapture_without_resurrecting() {
        let c = Control::default();
        capture(&c, Some(sample("A", 100)));
        capture(&c, Some(sample("B", 100)));
        c.mouse_at([20, 30]);
        let stale = c.request();
        c.remove("A");
        c.captured(stale, Some(sample("A", 100)));
        c.publish_for(stale, "A", Some(sample("A", 100)), String::new());
        assert_eq!(c.views().len(), 1);
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "B");
    }
    #[test]
    fn one_completion_or_error_does_not_affect_siblings() {
        let c = Control::default();
        capture(&c, Some(sample("A", 100)));
        capture(&c, Some(sample("B", 100)));
        let request = c.request();
        c.unavailable_for(request, "A", "temporary read error".into());
        c.publish_for(request, "A", Some(sample("unrelated", 0)), String::new());
        assert_eq!(c.views().len(), 2);
        assert!(c.views()[1].message.is_empty());
        c.publish_for(request, "A", Some(sample("A", 0)), String::new());
        assert_eq!(c.views().len(), 1);
        assert_eq!(c.view().snapshot.unwrap().recipe_id, "B");
        c.publish_for(request, "B", None, String::new());
        assert!(c.views().is_empty());
        assert!(!c.request().active);
    }
    #[test]
    fn new_request_rejects_stale_capture_and_preserves_current_generations() {
        let c = Control::default();
        capture(&c, Some(sample("A", 100)));
        c.mouse_at([20, 30]);
        let stale = c.request();
        c.mouse_at([30, 40]);
        c.captured(stale, Some(sample("B", 100)));
        assert_eq!(c.views().len(), 1);
        assert_eq!(c.view().snapshot.unwrap().generation, c.request().generation);
        c.captured(c.request(), None);
        assert!(c.request().cursor.is_none());
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
