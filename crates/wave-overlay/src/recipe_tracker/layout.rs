//! Group geometry in game-client pixels. Pause-menu layout reserves the right gutter.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    pub width: i32,
    pub gap: i32,
    pub height: i32,
    pub right_margin: i32,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub top_tab: bool,
    pub bottom_cut: bool,
}
/// Selection zero is the rightmost card; adornments belong to the outside edges.
pub(super) fn frame(index: usize, count: usize) -> Frame {
    Frame {
        top_tab: index + 1 == count,
        bottom_cut: index == 0,
    }
}
pub(super) const WIDTH: f64 = 96.0;
pub(super) fn control_y(width: i32) -> i32 {
    (10.0 * width as f64 / WIDTH).round().max(5.0) as i32 + 6
}
pub(super) fn controls(width: i32) -> f64 {
    (control_y(width) + super::close::SIZE + 6) as f64
}
/// Exact material-tile geometry shared by drawing and click-through hover tests.
pub(super) fn tile(width: i32, height: i32, count: usize, index: usize) -> [i32; 3] {
    let scale = width as f64 / WIDTH;
    let row = (((height as f64 - controls(width) - HEADER * scale) / count.max(1) as f64)
        .min(ROW * scale))
    .max(20.0);
    let size = (64.0 * scale).min(row - 8.0 * scale).round() as i32;
    [(width - size) / 2, (controls(width) + HEADER * scale + index as f64 * row) as i32, size]
}
const PAUSE_GUTTER: f64 = 0.096;
pub(super) const HEADER: f64 = 128.0;
pub(super) const ROW: f64 = 74.0;
pub(super) fn fit(
    client: [i32; 2],
    pins: usize,
    ingredients: usize,
    preference: u16,
    pause: bool,
) -> Option<Layout> {
    let [w, h] = client;
    if w <= 40 || h <= 40 || pins == 0 || pins > 3 {
        return None;
    }
    let requested = (h as f64 / 1440.0).clamp(0.65, 2.0) * preference as f64 / 100.0;
    let natural = HEADER + ROW * ingredients as f64 + 4.0;
    // The pause menu's right-side settings pane leaves a narrow outer gutter.
    // Other game UIs retain the full normal layout.
    let available = if pause {
        (w as f64 * PAUSE_GUTTER - 28.0).max(0.0)
    } else {
        w as f64 - 28.0
    };
    let group = WIDTH * pins as f64 + 6.0 * pins.saturating_sub(1) as f64;
    let width_fit = available / group;
    let height_fit = ((h - 80) as f64).max(0.0) / (natural + 10.0);
    let factor = requested.min(width_fit).min(height_fit);
    // Keep the group readable even when the normal preference is larger.
    if factor < requested.min(0.65) {
        return None;
    }
    let width = (WIDTH * factor).floor() as i32;
    let gap = if pins > 1 && width_fit < requested && width_fit <= height_fit {
        // Use the available strip only when width actually forces a reduction.
        // Absorb whole-pixel rounding into the gaps, keeping both edge clearances.
        ((available.floor() as i32 - width * pins as i32) / (pins as i32 - 1)).max(3)
    } else {
        (6.0 * factor).floor().max(3.0) as i32
    };
    if width * pins as i32 + gap * pins.saturating_sub(1) as i32 > available.floor() as i32 {
        return None;
    }
    Some(Layout {
        width,
        gap,
        height: controls(width) as i32 + (natural * width as f64 / WIDTH).round() as i32,
        right_margin: 14,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_all_pins_as_one_group_and_restores_normal_size() {
        let normal = fit([2560, 1440], 3, 5, 100, false).unwrap();
        let pause = fit([2560, 1440], 3, 5, 100, true).unwrap();
        assert!(pause.width < normal.width);
        assert!(
            pause.width > 67 && pause.width < 75,
            "Pause size should use the available strip without crowding the menu"
        );
        assert_eq!(normal.width, 96);
        let group = pause.width * 3 + pause.gap * 2;
        let left = 2560.0 * PAUSE_GUTTER - group as f64 - pause.right_margin as f64;
        assert!(
            (14.0..16.0).contains(&left),
            "Scaled pause clearance must match the fixed right edge within pixel rounding"
        );
        assert!(control_y(normal.width) >= 16);
        assert_eq!(fit([2560, 1440], 3, 5, 150, true).unwrap().width, pause.width);
        assert!(pause.width * 3 + pause.gap * 2 <= (2560.0 * PAUSE_GUTTER - 28.0) as i32);
        assert_eq!(fit([2560, 1440], 3, 5, 100, false).unwrap().width, 96);
    }
    #[test]
    fn right_edge_is_fixed_and_small_groups_do_not_shrink_or_recentre() {
        for client in [[2560, 1440], [3840, 2160]] {
            for pins in 1..=3 {
                let normal = fit(client, pins, 5, 100, false).unwrap();
                let pause = fit(client, pins, 5, 100, true).unwrap();
                assert_eq!(normal.right_margin, 14);
                assert_eq!(pause.right_margin, 14);
                if pins < 3 {
                    assert_eq!(pause.width, normal.width);
                    assert_eq!(pause.gap, normal.gap);
                } else {
                    assert!(pause.width < normal.width);
                    let left = client[0] as f64 * PAUSE_GUTTER
                        - (pause.width * pins as i32 + pause.gap * (pins as i32 - 1) + 14) as f64;
                    assert!((14.0..16.0).contains(&left));
                }
            }
        }
    }
    #[test]
    fn unreadable_or_invalid_groups_hide_instead_of_clipping() {
        assert!(fit([1280, 720], 3, 5, 150, true).is_none());
        assert!(fit([2560, 1440], 4, 5, 100, false).is_none());
        assert!(fit([0, 0], 1, 5, 100, false).is_none());
        assert!(fit([2560, 1440], 2, 5, 100, true).is_some());
        assert!(fit([2560, 720], 3, 20, 100, false).is_none());
    }
}
