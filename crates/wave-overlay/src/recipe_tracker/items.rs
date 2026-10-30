//! Click-through material hover and a short, owned item-name tooltip. No game calls.
use super::*;
use std::time::{Duration, Instant};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub panel: usize,
    pub ingredient: usize,
    pub center: [i32; 2],
    recipe: String,
    pub item: String,
    generation: u64,
}
pub(super) fn hit(
    views: &[View],
    geometry: layout::Layout,
    right: i32,
    top: i32,
    point: [i32; 2],
) -> Option<Target> {
    for (panel, view) in views.iter().enumerate() {
        let snapshot = view.snapshot.as_ref()?;
        let left = right - geometry.width - (geometry.width + geometry.gap) * panel as i32;
        for (ingredient, item) in snapshot.ingredients.iter().enumerate() {
            let [x, y, size] = layout::tile(
                geometry.width,
                geometry.height,
                snapshot.ingredients.len(),
                ingredient,
            );
            if (left + x..left + x + size).contains(&point[0])
                && (top + y..top + y + size).contains(&point[1])
            {
                return Some(Target {
                    panel,
                    ingredient,
                    center: [left + x + size / 2, top + y + size / 2],
                    recipe: snapshot.recipe_id.clone(),
                    item: item.item_id.clone(),
                    generation: view.generation,
                });
            }
        }
    }
    None
}
fn tooltip_position(center: [i32; 2], size: [i32; 2], bounds: [i32; 4]) -> [i32; 2] {
    let [left, top, right, bottom] = bounds;
    let x = if center[0] + size[0] <= right - 6 {
        center[0]
    } else {
        center[0] - size[0]
    };
    let y = if center[1] + size[1] <= bottom - 6 {
        center[1]
    } else {
        center[1] - size[1]
    };
    [
        x.clamp(left + 6, (right - 6 - size[0]).max(left + 6)),
        y.clamp(top + 6, (bottom - 6 - size[1]).max(top + 6)),
    ]
}
pub(super) struct Hover {
    window: HWND,
    target: Option<Target>,
    since: Instant,
    content: Option<(String, i32, i32)>,
    size: [i32; 2],
    position: Option<[i32; 2]>,
    anchor: HWND,
    bounds: Option<[i32; 4]>,
}
impl Default for Hover {
    fn default() -> Self {
        Self {
            window: std::ptr::null_mut(),
            target: None,
            since: Instant::now(),
            content: None,
            size: [0, 0],
            position: None,
            anchor: std::ptr::null_mut(),
            bounds: None,
        }
    }
}
impl Drop for Hover {
    fn drop(&mut self) {
        if !self.window.is_null() {
            unsafe {
                DestroyWindow(self.window);
            }
        }
    }
}
impl Hover {
    fn hide_window(&self) {
        if !self.window.is_null() {
            unsafe {
                ShowWindow(self.window, SW_HIDE);
            }
        }
    }
    pub(super) fn owns(&self, window: HWND) -> bool {
        !window.is_null() && self.window == window
    }
    pub(super) fn hide(&mut self) {
        self.target = None;
        self.position = None;
        self.hide_window();
    }
    pub(super) fn select(&mut self, target: Option<Target>, now: Instant) -> bool {
        if target != self.target {
            self.target = target;
            self.since = now;
            self.position = None;
            self.hide_window();
        }
        self.target.is_some()
            && now.saturating_duration_since(self.since) >= Duration::from_millis(350)
    }
    fn create(&mut self) -> Result<()> {
        if !self.window.is_null() {
            return Ok(());
        }
        let class = wide("StarRuptureItemTooltip");
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        }
        self.window = unsafe {
            CreateWindowExW(
                crate::stacking::EXTENDED_STYLE,
                class.as_ptr(),
                wide("Recipe item name").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                std::ptr::null(),
            )
        };
        ensure!(!self.window.is_null(), "Could not create item tooltip");
        Ok(())
    }
    pub(super) fn show(
        &mut self,
        name: &str,
        center: [i32; 2],
        bounds: [i32; 4],
        font: i32,
        game: HWND,
        anchor: HWND,
    ) -> Result<()> {
        self.create()?;
        let max_width = ((bounds[2] - bounds[0] - 12).min(font * 20)).max(40);
        let key = (name.to_owned(), font, max_width);
        if self.content.as_ref() != Some(&key) {
            self.size = render::tooltip(self.window, name, font, max_width)?;
            self.content = Some(key);
            self.position = None;
        }
        if self.bounds != Some(bounds) {
            self.position = None;
            self.bounds = Some(bounds);
        }
        if self.position.is_none() {
            let position = tooltip_position(center, self.size, bounds);
            unsafe {
                SetWindowPos(
                    self.window,
                    std::ptr::null_mut(),
                    position[0],
                    position[1],
                    self.size[0],
                    self.size[1],
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            self.position = Some(position);
        }
        self.anchor = anchor;
        crate::stacking::follow_companion(self.window, anchor, game);
        Ok(())
    }
    pub(super) fn follow(&mut self, game: HWND) {
        if unsafe { GetForegroundWindow() } != game {
            self.hide();
            return;
        }
        if !self.window.is_null() && unsafe { IsWindowVisible(self.window) } != 0 {
            crate::stacking::follow_companion(self.window, self.anchor, game);
        }
    }
}
pub(super) fn self_test() -> Result<()> {
    let foreground = unsafe { GetForegroundWindow() };
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    let class = wide("StarRuptureTooltipFixture");
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: module,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&wc);
    }
    let make = || -> Result<Window> {
        let w = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                wide("Owned tooltip fixture").as_ptr(),
                WS_POPUP,
                -32000,
                -32000,
                1000,
                600,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                std::ptr::null(),
            )
        };
        ensure!(!w.is_null(), "Tooltip fixture");
        unsafe {
            ShowWindow(w, SW_SHOWNOACTIVATE);
        }
        Ok(Window(w))
    };
    let game = make()?;
    let panel = make()?;
    ensure!(crate::stacking::follow_game(panel.0, game.0), "Tooltip fixture ordering");
    let mut hover = Hover::default();
    hover.show(
        "Calcite Sheets",
        [-31010, -31410],
        [-32000, -32000, -31000, -31400],
        18,
        game.0,
        panel.0,
    )?;
    ensure!(
        unsafe { GetWindowLongPtrW(hover.window, GWL_EXSTYLE) } as u32 & WS_EX_TRANSPARENT != 0,
        "Tooltip intercepts game input"
    );
    ensure!(
        unsafe { GetWindowLongPtrW(hover.window, GWL_EXSTYLE) } as u32 & WS_EX_NOACTIVATE != 0,
        "Tooltip can activate"
    );
    ensure!(
        unsafe { GetWindow(panel.0, GW_HWNDPREV) } == hover.window,
        "Tooltip is behind its card"
    );
    let size = hover.size;
    let position = hover.position;
    hover.show(
        "Calcite Sheets",
        [-31020, -31420],
        [-32000, -32000, -31000, -31400],
        18,
        game.0,
        panel.0,
    )?;
    ensure!(hover.size == size, "Pointer movement changed tooltip size");
    ensure!(hover.position == position, "Pointer movement moved the open tooltip");
    hover.hide();
    ensure!(unsafe { IsWindowVisible(hover.window) } == 0, "Tooltip did not hide");
    let window = hover.window;
    drop(hover);
    ensure!(unsafe { IsWindow(window) } == 0, "Tooltip HWND leaked");
    ensure!(unsafe { GetForegroundWindow() } == foreground, "Tooltip stole focus");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> View {
        View {
            snapshot: Some(starrupture_observation_model::recipe::Snapshot {
                recipe_id: "recipe".into(),
                ingredients: vec![starrupture_observation_model::recipe::Ingredient {
                    item_id: "material".into(),
                    submitted: 0,
                    required: 10,
                    remaining: 10,
                    bag_count: None,
                }],
                observed_at_ms: 1,
                generation: 4,
                process: 1,
            }),
            generation: 4,
            ..View::default()
        }
    }
    #[test]
    fn hover_targets_only_the_actual_tile_at_normal_and_pause_scales() {
        let views = [view()];
        for pause in [false, true] {
            let g = layout::fit([2560, 1440], 1, 1, 100, pause).unwrap();
            let [x, y, size] = layout::tile(g.width, g.height, 1, 0);
            let left = 1000 - g.width;
            let target = hit(&views, g, 1000, 30, [left + x + 1, 30 + y + 1]).unwrap();
            assert_eq!(target.item, "material");
            assert_eq!(target.center, [left + x + size / 2, 30 + y + size / 2]);
            assert_eq!(
                Some(target),
                hit(&views, g, 1000, 30, [left + x + size - 1, 30 + y + size - 1])
            );
            assert!(hit(&views, g, 1000, 30, [left + x + size, 30 + y + 1]).is_none());
            assert!(hit(&views, g, 1000, 30, [left + x + 1, 30 + y - 1]).is_none());
        }
    }
    #[test]
    fn delay_resets_on_item_or_selection_change_and_leave() {
        let mut h = Hover::default();
        let now = Instant::now();
        let g = layout::fit([2560, 1440], 1, 1, 100, false).unwrap();
        let [x, y, _] = layout::tile(g.width, g.height, 1, 0);
        let t = hit(&[view()], g, 1000, 0, [1000 - g.width + x + 1, y + 1]);
        assert!(!h.select(t.clone(), now));
        assert!(!h.select(t.clone(), now + Duration::from_millis(349)));
        assert!(h.select(t.clone(), now + Duration::from_millis(350)));
        h.position = Some([100, 100]);
        assert!(h.select(t.clone(), now + Duration::from_millis(400)));
        assert_eq!(h.position, Some([100, 100]));
        let mut changed = t.unwrap();
        changed.generation += 1;
        assert!(!h.select(Some(changed), now + Duration::from_secs(1)));
        assert!(h.position.is_none());
        h.hide();
        assert!(h.target.is_none());
    }
    #[test]
    fn tooltip_flips_and_stays_inside_the_game() {
        assert_eq!(tooltip_position([200, 200], [180, 40], [0, 0, 800, 600]), [200, 200]);
        assert_eq!(tooltip_position([750, 200], [180, 40], [0, 0, 800, 600]), [570, 200]);
        for point in [[8, 8], [790, 590], [790, 8], [8, 590]] {
            let [x, y] = tooltip_position(point, [180, 40], [0, 0, 800, 600]);
            assert!(x >= 6 && y >= 6 && x + 180 <= 794 && y + 40 <= 594);
        }
    }
}
