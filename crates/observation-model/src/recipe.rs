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
pub fn valid_card(id: &str) -> bool {
    if let Some(building) = id.strip_prefix("upgrade:") {
        return crate::codex::asset_id(building);
    }
    if let Some(tier) = id.strip_prefix("core-tier:") {
        return tier.parse::<u32>().is_ok_and(|tier| (1..64).contains(&tier));
    }
    if let Some(core) = id.strip_prefix("core:") {
        return core.split_once(':').is_some_and(|(persistent, tier)| {
            persistent.parse::<u32>().is_ok_and(|id| id != 0)
                && tier.parse::<u32>().is_ok_and(|tier| (1..64).contains(&tier))
        });
    }
    crate::codex::asset_id(id)
}
pub const MAX_PINS: usize = 3;
pub const PIN_LIMIT_NOTICE: &str = "Three recipes are pinned. Close one before adding another.";
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
    edited: bool,
    request: Request,
    catalogue_selection: Option<String>,
    pins: Vec<View>,
    cards: Vec<(String, u32)>,
    retained: BTreeSet<String>,
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
        self.catalogue_selection = None;
        for view in &mut self.pins {
            view.generation = self.request.generation;
            if let Some(snapshot) = &mut view.snapshot {
                snapshot.generation = view.generation;
            }
        }
    }
    fn settle(&mut self) {
        self.catalogue_selection = None;
        self.request.cursor = None;
        self.request.active = !self.pins.is_empty();
    }
}
impl Control {
    pub fn request(&self) -> Request {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).request
    }
    /// Queue an exact recipe for a fresh world-bound research read.
    pub fn select_catalogue(&self, id: &str) {
        if !crate::codex::asset_id(id) {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.pins.iter().any(|view| view.snapshot.as_ref().is_some_and(|s| s.recipe_id == id)) {
            return;
        }
        state.advance();
        state.request.active = true;
        state.request.cursor = None;
        state.request.clear_outside = false;
        state.edited = true;
        state.catalogue_selection = Some(id.into());
    }
    pub fn reject_catalogue(&self, request: Request, notice: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request == request && state.catalogue_selection.is_some() {
            state.settle();
            state.notice = notice;
        }
    }
    pub fn catalogue_selection(&self, request: Request) -> Option<String> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request == request {
            state.catalogue_selection.clone()
        } else {
            None
        }
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
        state.edited = true;
    }
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.advance();
        state.pins.clear();
        state.cards.clear();
        state.retained.clear();
        state.edited = true;
        state.settle();
    }
    /// Clear volatile observations at a new session, leaving restoration eligible.
    pub fn reset_session(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.advance();
        state.pins.clear();
        state.cards.clear();
        state.retained.clear();
        state.edited = false;
        state.settle();
    }
    pub fn restoration_request(&self) -> Option<Request> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (!state.edited && state.pins.is_empty() && !state.request.active).then_some(state.request)
    }
    /// Only fresh guarded snapshots may be restored, and never over a user's action.
    pub fn restore(&self, request: Request, snapshots: Vec<Snapshot>) -> bool {
        self.restore_catalogue(request, snapshots, Vec::new(), BTreeSet::new())
    }

    pub fn restore_catalogue(
        &self,
        request: Request,
        snapshots: Vec<Snapshot>,
        cards: Vec<(String, u32)>,
        retained: BTreeSet<String>,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut ids = BTreeSet::new();
        if state.edited
            || state.request != request
            || !state.pins.is_empty()
            || state.request.active
            || snapshots.len() + cards.len() > MAX_PINS
            || snapshots.iter().any(|s| s.recipe_id.is_empty() || !ids.insert(&s.recipe_id))
            || cards.iter().any(|(id, pid)| *pid == 0 || !valid_card(id) || !ids.insert(id))
        {
            return false;
        }
        state.advance();
        state.edited = true;
        state.cards = cards;
        state.retained = retained;
        for mut snapshot in
            snapshots.into_iter().filter(|s| s.ingredients.iter().any(|i| i.remaining > 0))
        {
            snapshot.generation = state.request.generation;
            let generation = state.request.generation;
            state.pins.push(View {
                snapshot: Some(snapshot),
                message: String::new(),
                generation,
            });
        }
        state.settle();
        true
    }
    pub fn pin_card(&self, id: &str, process: u32) {
        if process == 0 || !valid_card(id) {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cards.iter().any(|(key, _)| key == id)
            || state.pins.iter().any(|v| v.snapshot.as_ref().is_some_and(|s| s.recipe_id == id))
        {
            return;
        }
        state.advance();
        if state.pins.len() + state.cards.len() < MAX_PINS {
            state.cards.push((id.into(), process));
            state.edited = true;
        } else {
            state.notice = PIN_LIMIT_NOTICE.into();
        }
        state.settle();
    }

    pub fn remembered_ids(&self) -> Option<Vec<String>> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request.cursor.is_some() || state.catalogue_selection.is_some() {
            return None;
        }
        let mut ids: Vec<_> = state
            .pins
            .iter()
            .filter_map(|view| view.snapshot.as_ref())
            .map(|snap| {
                if state.retained.contains(&snap.recipe_id) {
                    format!("#tracked:{}", snap.recipe_id)
                } else {
                    snap.recipe_id.clone()
                }
            })
            .collect();
        ids.extend(state.cards.iter().map(|(id, _)| format!("#card:{id}")));
        Some(ids)
    }

    pub fn settle_catalogue_as_card(&self, request: Request, pid: u32) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.request != request || pid == 0 {
            return;
        }
        if let Some(id) = state.catalogue_selection.take() {
            if state.pins.len() + state.cards.len() < MAX_PINS {
                state.cards.push((id, pid));
            } else {
                state.notice = PIN_LIMIT_NOTICE.into();
            }
            state.settle();
        }
    }
    pub fn card_ids(&self, process: u32) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cards
            .iter()
            .filter(|(_, pid)| *pid == process)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn remove(&self, id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.cards.iter().any(|(key, _)| key == id)
            && !state.pins.iter().any(|p| p.snapshot.as_ref().is_some_and(|s| s.recipe_id == id))
        {
            return;
        }
        state.advance();
        state.cards.retain(|(key, _)| key != id);
        state.retained.remove(id);
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
                if state.retained.remove(id)
                    && let Some(previous) = &state.pins[index].snapshot
                {
                    let process = previous.process;
                    state.cards.push((id.into(), process));
                }
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
            state.cards.clear();
            state.retained.clear();
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
                state.retained.remove(&next.recipe_id);
                state.pins.remove(index);
            } else if let Some(index) = state.cards.iter().position(|(id, _)| id == &next.recipe_id)
            {
                state.cards.remove(index);
            } else if state.pins.len() + state.cards.len() < MAX_PINS {
                if state.catalogue_selection.as_deref() == Some(next.recipe_id.as_str()) {
                    state.retained.insert(next.recipe_id.clone());
                }
                next.generation = request.generation;
                state.pins.push(View {
                    snapshot: Some(next),
                    message: String::new(),
                    generation: request.generation,
                });
            } else {
                state.notice = PIN_LIMIT_NOTICE.into();
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
    pub fn pinned_ids(&self, process: u32) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pins
            .iter()
            .filter_map(|view| view.snapshot.as_ref().filter(|s| s.process == process))
            .map(|snapshot| snapshot.recipe_id.clone())
            .collect()
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
    #[test]
    fn upgrade_cards_share_capacity_and_persist_without_live_counts() {
        let control = super::Control::default();
        for id in ["core:42:2", "upgrade:/Game/Chimera/B_A", "/Game/Chimera/Recipe", "core:43:2"] {
            control.pin_card(id, 7);
        }
        assert_eq!(control.card_ids(7).len(), 3);
        assert!(control.card_ids(8).is_empty());
        assert!(control.notice().contains("Three recipes"));
        control.remove("core:42:2");
        assert_eq!(
            control.remembered_ids().unwrap(),
            ["#card:upgrade:/Game/Chimera/B_A", "#card:/Game/Chimera/Recipe"]
        );
        control.reset_session();
        assert!(control.card_ids(7).is_empty());
        let request = control.restoration_request().unwrap();
        assert!(control.restore_catalogue(
            request,
            Vec::new(),
            vec![("core:42:2".into(), 8)],
            Default::default()
        ));
        assert_eq!(control.card_ids(8), ["core:42:2"]);
    }
    #[test]
    fn catalogue_requests_do_not_replace_or_clear_game_shortcut_requests() {
        let control = super::Control::default();
        let id = "/Game/Chimera/Recipe_Test";
        control.select_catalogue(id);
        let codex = control.request();
        assert!(codex.active && codex.cursor.is_none() && !codex.clear_outside);
        assert_eq!(control.catalogue_selection(codex).as_deref(), Some(id));
        control.toggle_at(Some([1, 2]));
        let game = control.request();
        assert_eq!(control.catalogue_selection(codex), None);
        assert_eq!(control.catalogue_selection(game), None);
        control.reject_catalogue(codex, "Stale".into());
        assert_eq!(control.request(), game);
    }
    #[test]
    fn restoration_refreshes_generation_and_respects_manual_actions() {
        let c = Control::default();
        let request = c.restoration_request().unwrap();
        assert!(c.restore(request, vec![sample("A", 10), sample("Done", 0)]));
        assert_eq!(c.views().len(), 1);
        assert!(c.restoration_request().is_none());
        let v = c.view();
        assert_eq!(v.snapshot.unwrap().generation, v.generation);
        c.reset_session();
        let request = c.restoration_request().unwrap();
        c.toggle_at(Some([1, 2]));
        assert!(!c.restore(request, vec![sample("Old", 10)]));
        c.reset_session();
        let request = c.restoration_request().unwrap();
        c.clear();
        assert!(!c.restore(request, vec![sample("Old", 10)]));
        assert!(c.views().is_empty());
    }
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
