//! Personal-waypoint silhouette, expressed as a filled vector outline.
//! Keeps the angular flag, open lower return and slanted base at their native proportions.
pub(super) fn mask(size: usize) -> Vec<u8> {
    const OUTLINE: [[f64; 2]; 16] = [
        [30., 6.],
        [97., 6.],
        [128., 37.],
        [95., 70.],
        [68., 70.],
        [52., 54.],
        [90., 54.],
        [106., 38.],
        [90., 22.],
        [46., 22.],
        [46., 106.],
        [109., 106.],
        [109., 122.],
        [0., 122.],
        [16., 106.],
        [30., 106.],
    ];
    let inside = |x: f64, y: f64| {
        let mut filled = false;
        let mut previous = OUTLINE[OUTLINE.len() - 1];
        for current in OUTLINE {
            if (current[1] > y) != (previous[1] > y)
                && x < (previous[0] - current[0]) * (y - current[1]) / (previous[1] - current[1])
                    + current[0]
            {
                filled = !filled;
            }
            previous = current;
        }
        filled
    };
    let mut pixels = vec![0; size * size];
    for y in 0..size {
        for x in 0..size {
            let mut coverage = 0u32;
            for dy in 0..4 {
                for dx in 0..4 {
                    let sx = (x as f64 + (f64::from(dx) + 0.5) / 4.) * 128. / size as f64;
                    let sy = (y as f64 + (f64::from(dy) + 0.5) / 4.) * 128. / size as f64;
                    coverage += u32::from(inside(sx, sy));
                }
            }
            pixels[y * size + x] = ((coverage * 255 + 8) / 16) as u8;
        }
    }
    pixels
}
