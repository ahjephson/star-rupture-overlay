//! CPU image composition followed by GDI text, with no concurrent pixel aliases.
use super::*;
fn color(c: [u8; 3]) -> u32 {
    c[0] as u32 | ((c[1] as u32) << 8) | ((c[2] as u32) << 16)
}
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
impl Surface {
    fn new(width: i32, height: i32) -> Result<Self> {
        ensure!(
            (1..=4096).contains(&width) && (1..=4096).contains(&height),
            "Recipe surface bounds"
        );
        let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        ensure!(!dc.is_null(), "Recipe render context");
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0)
        };
        if bitmap.is_null() {
            unsafe {
                DeleteDC(dc);
            }
            anyhow::bail!("Recipe render bitmap")
        };
        let old = unsafe { SelectObject(dc, bitmap) };
        if old.is_null() || old as isize == -1 {
            unsafe {
                DeleteObject(bitmap);
                DeleteDC(dc);
            }
            anyhow::bail!("Recipe bitmap selection")
        }
        Ok(Self {
            dc,
            bitmap,
            old,
            bits: bits.cast(),
            width,
            height,
        })
    }
    fn pixels(&self) -> Vec<u8> {
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts(self.bits, self.width as usize * self.height as usize * 4)
                .to_vec()
        }
    }
}
fn image(
    pixels: &mut [u8],
    dimensions: [usize; 2],
    img: &Image,
    x: i32,
    y: i32,
    size: i32,
    tint: Option<[u8; 3]>,
) {
    image_rect(pixels, dimensions, img, [x, y, size, size], tint);
}
fn image_rect(
    pixels: &mut [u8],
    dimensions: [usize; 2],
    img: &Image,
    rect: [i32; 4],
    tint: Option<[u8; 3]>,
) {
    let [x, y, draw_width, draw_height] = rect;
    let [w, h] = dimensions;
    if draw_width <= 0 || draw_height <= 0 {
        return;
    }
    for oy in 0..draw_height {
        let dy = y + oy;
        if dy < 0 || dy >= h as i32 {
            continue;
        }
        let sy = (oy as usize * img.height / draw_height as usize).min(img.height - 1);
        for ox in 0..draw_width {
            let dx = x + ox;
            if dx < 0 || dx >= w as i32 {
                continue;
            }
            let sx = (ox as usize * img.width / draw_width as usize).min(img.width - 1);
            let s = &img.pixels[(sy * img.width + sx) * 4..][..4];
            let d = &mut pixels[(dy as usize * w + dx as usize) * 4..][..4];
            let a = s[3] as u32;
            for c in 0..3 {
                let value = tint.map_or(s[c] as u32, |t| {
                    if t[0] == t[1] && t[1] == t[2] {
                        let luma = (s[2] as u32 * 54 + s[1] as u32 * 183 + s[0] as u32 * 19) / 256;
                        luma * t[0] as u32 / 255
                    } else {
                        s[c] as u32 * t[2 - c] as u32 / 255
                    }
                });
                d[c] = (value + d[c] as u32 * (255 - a) / 255).min(255) as u8
            }
            d[3] = (a + d[3] as u32 * (255 - a) / 255).min(255) as u8;
        }
    }
}
enum TextLayout {
    Centered,
    Counter {
        line_advance: i32,
    },
}
struct Text {
    value: String,
    rect: RECT,
    size: i32,
    colour: [u8; 3],
    outline: bool,
    layout: TextLayout,
}
impl Text {
    fn draw(&self, dc: HDC) {
        unsafe {
            let name = wide("Nippo");
            let mut size = self.size;
            let mut selected = std::ptr::null_mut();
            let mut font = std::ptr::null_mut();
            while size >= 8 {
                font = CreateFontW(
                    -size,
                    0,
                    0,
                    0,
                    400,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    ANTIALIASED_QUALITY as u32,
                    DEFAULT_PITCH as u32,
                    name.as_ptr(),
                );
                if font.is_null() {
                    return;
                }
                selected = SelectObject(dc, font);
                let mut longest = 0;
                for line in self.value.lines() {
                    let line = wide(line);
                    let mut extent = SIZE::default();
                    GetTextExtentPoint32W(dc, line.as_ptr(), line.len() as i32 - 1, &mut extent);
                    longest = longest.max(extent.cx);
                }
                let mut measured = RECT {
                    left: 0,
                    top: 0,
                    right: self.rect.right - self.rect.left,
                    bottom: 0,
                };
                let value = wide(&self.value);
                let fits_height = match self.layout {
                    TextLayout::Centered => {
                        DrawTextW(
                            dc,
                            value.as_ptr(),
                            value.len() as i32 - 1,
                            &mut measured,
                            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
                        );
                        measured.bottom <= self.rect.bottom - self.rect.top
                    }
                    TextLayout::Counter {
                        ..
                    } => true,
                };
                if longest <= self.rect.right - self.rect.left - 4 && fits_height || size == 8 {
                    break;
                }
                SelectObject(dc, selected);
                DeleteObject(font);
                size -= 1;
            }
            SetBkMode(dc, TRANSPARENT as i32);
            let draw_line = |value: &str, rect: RECT, flags| {
                let value = wide(value);
                if self.outline {
                    SetTextColor(dc, 0);
                    let mut shadow = RECT {
                        left: rect.left + 2,
                        top: rect.top + 2,
                        right: rect.right + 2,
                        bottom: rect.bottom + 2,
                    };
                    DrawTextW(dc, value.as_ptr(), value.len() as i32 - 1, &mut shadow, flags);
                    for (x, y) in
                        [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]
                    {
                        let mut r = RECT {
                            left: rect.left + x,
                            top: rect.top + y,
                            right: rect.right + x,
                            bottom: rect.bottom + y,
                        };
                        DrawTextW(dc, value.as_ptr(), value.len() as i32 - 1, &mut r, flags);
                    }
                }
                SetTextColor(dc, color(self.colour));
                let mut r = rect;
                DrawTextW(dc, value.as_ptr(), value.len() as i32 - 1, &mut r, flags);
            };
            match self.layout {
                TextLayout::Centered => {
                    let flags = DT_CENTER
                        | DT_NOPREFIX
                        | if self.outline && !self.value.contains('\n') {
                            DT_SINGLELINE
                        } else {
                            DT_WORDBREAK
                        };
                    draw_line(&self.value, self.rect, flags);
                }
                TextLayout::Counter {
                    line_advance,
                } => {
                    // Slate's bottom-aligned Nippo counter uses a wider line advance than GDI.
                    for (index, line) in self.value.lines().enumerate() {
                        let rect = RECT {
                            top: self.rect.top + index as i32 * line_advance,
                            ..self.rect
                        };
                        draw_line(line, rect, DT_LEFT | DT_NOPREFIX | DT_SINGLELINE);
                    }
                }
            }
            SelectObject(dc, selected);
            DeleteObject(font);
        }
    }
}
fn draw(surface: &Surface, assets: &Assets, view: &View, now: u64) {
    draw_panel(surface, assets, view, now, layout::frame(0, 1), None);
}
fn draw_panel(
    surface: &Surface,
    assets: &Assets,
    view: &View,
    now: u64,
    frame: layout::Frame,
    hovered: Option<usize>,
) {
    let width = surface.width;
    let height = surface.height;
    let w = width as usize;
    let h = height as usize;
    let mut pixels = vec![0; w * h * 4];
    for p in pixels.as_chunks_mut::<4>().0 {
        p.copy_from_slice(&[29, 27, 24, 235]);
    }
    let mut texts = Vec::new();
    let count = view.snapshot.as_ref().map_or(0, |s| s.ingredients.len());
    let scale = width as f64 / layout::WIDTH;
    let cat = &assets.catalogue;
    let fresh = view
        .snapshot
        .as_ref()
        .is_some_and(|s| s.observed_at_ms <= now && now - s.observed_at_ms <= 5000)
        && view.message.is_empty();
    if let Some(snapshot) = &view.snapshot {
        if let Some(recipe) = cat.recipes.get(&snapshot.recipe_id) {
            let header = (62.0 * scale) as i32;
            let hx = (width - header) / 2;
            let hy = layout::controls(width) as i32;
            let frame_rect = [hx, hy, header, (74.0 * scale).round() as i32];
            image_rect(&mut pixels, [w, h], &assets.header_background, frame_rect, None);
            image(
                &mut pixels,
                [w, h],
                &assets.images[&cat.items[&recipe.item_id].image],
                hx + (2.0 * scale) as i32,
                hy + (2.0 * scale) as i32,
                (58.0 * scale) as i32,
                None,
            );
            image_rect(&mut pixels, [w, h], &assets.header_frame, frame_rect, None);
            texts.push(Text {
                value: recipe.name.replacen(' ', "\n", 1),
                rect: RECT {
                    left: 8,
                    top: layout::controls(width) as i32 + (80.0 * scale) as i32,
                    right: width - 8,
                    bottom: layout::controls(width) as i32 + (120.0 * scale) as i32,
                },
                size: (14.0 * scale) as i32,
                colour: [240, 240, 235],
                outline: false,
                layout: TextLayout::Centered,
            });
            for (index, item) in snapshot.ingredients.iter().enumerate() {
                let [x, y, tile] = layout::tile(width, height, count, index);
                let complete = fresh && item.remaining == 0;
                let mut bg: [u8; 4] = if complete {
                    [55, 55, 55, 255]
                } else {
                    [107, 119, 133, 255]
                };
                if hovered == Some(index) {
                    for component in &mut bg[..3] {
                        *component = component.saturating_add(22);
                    }
                }
                for py in y.max(0)..(y + tile).min(height) {
                    for px in x.max(0)..(x + tile).min(width) {
                        pixels[(py as usize * w + px as usize) * 4..][..4].copy_from_slice(&bg)
                    }
                }
                if let Some(def) = cat.items.get(&item.item_id) {
                    image(
                        &mut pixels,
                        [w, h],
                        &assets.images[&def.image],
                        x + 2,
                        y + 2,
                        tile - 4,
                        if complete {
                            Some([110, 110, 110])
                        } else {
                            None
                        },
                    );
                }
                if complete {
                    let tick = (27.0 * scale) as i32;
                    image(
                        &mut pixels,
                        [w, h],
                        &assets.images[&cat.tick],
                        x + tile - tick,
                        y,
                        tick,
                        Some([117, 224, 230]),
                    );
                } else {
                    texts.push(Text {
                        value: if fresh {
                            if item.submitted > 0 && item.remaining > 0 {
                                format!("{} /\n{}", item.submitted, item.required)
                            } else {
                                format!("{} / {}", item.submitted, item.required)
                            }
                        } else {
                            format!("— / {}", item.required)
                        },
                        rect: RECT {
                            left: x + (5.0 * scale).round() as i32,
                            top: y + tile
                                - (if fresh && item.submitted > 0 && item.remaining > 0 {
                                    48.0
                                } else {
                                    28.0
                                } * scale) as i32,
                            right: x + tile,
                            bottom: y + tile,
                        },
                        size: (18.0 * scale).max(10.0) as i32,
                        colour: if fresh {
                            match item.bag_count {
                                Some(n) if n > 0 => [240, 240, 240],
                                Some(_) => [255, 0, 0],
                                None => [180, 180, 180],
                            }
                        } else {
                            [180, 180, 180]
                        },
                        outline: true,
                        layout: TextLayout::Counter {
                            line_advance: (24.0 * scale).round() as i32,
                        },
                    });
                    if fresh && let Some(n) = item.bag_count.filter(|n| *n > 0) {
                        texts.push(Text {
                            value: format!("[{n}]"),
                            rect: RECT {
                                left: x + (20.0 * scale) as i32,
                                top: y + 1,
                                right: x + tile - 1,
                                bottom: y + (18.0 * scale) as i32,
                            },
                            size: (12.0 * scale).max(9.0) as i32,
                            colour: [255, 163, 0],
                            outline: true,
                            layout: TextLayout::Centered,
                        });
                    }
                }
            }
            if !fresh {
                texts.push(Text {
                    value: "Reading unavailable".into(),
                    rect: RECT {
                        left: 4,
                        top: height - (17.0 * scale) as i32,
                        right: width - 4,
                        bottom: height - 2,
                    },
                    size: (11.0 * scale) as i32,
                    colour: [180, 180, 180],
                    outline: false,
                    layout: TextLayout::Centered,
                });
            }
        }
    } else {
        texts.push(Text {
            value: "Recipe tracker".into(),
            rect: RECT {
                left: 8,
                top: 20,
                right: width - 8,
                bottom: 50,
            },
            size: (18.0 * scale) as i32,
            colour: [235, 235, 235],
            outline: false,
            layout: TextLayout::Centered,
        });
        texts.push(Text {
            value: view.message.clone(),
            rect: RECT {
                left: 12,
                top: 55,
                right: width - 12,
                bottom: height - 8,
            },
            size: (14.0 * scale) as i32,
            colour: [200, 200, 200],
            outline: false,
            layout: TextLayout::Centered,
        });
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), surface.bits, pixels.len());
    }
    for text in texts {
        text.draw(surface.dc)
    }
    unsafe {
        GdiFlush();
        std::ptr::copy_nonoverlapping(surface.bits, pixels.as_mut_ptr(), pixels.len());
    }
    let cut = (10.0 * scale).round().max(5.0) as usize;
    for y in 0..h {
        for x in 0..w {
            let p = &mut pixels[(y * w + x) * 4..][..4];
            let top = if frame.top_tab {
                x.saturating_sub(w / 3).min(cut)
            } else {
                cut
            };
            let diagonal = w + h
                - 2
                - if frame.bottom_cut {
                    cut
                } else {
                    0
                };
            if y < top || x + y > diagonal {
                p.fill(0);
                continue;
            }
            if x < 2 || x + 2 >= w || y + 2 >= h || y - top < 2 || diagonal - (x + y) < 2 {
                p.copy_from_slice(&[218, 222, 220, 255]);
            } else if p[3] == 0 {
                p[3] = 255;
            }
        }
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), surface.bits, pixels.len());
    }
}
pub(super) fn render(
    window: HWND,
    assets: &Assets,
    view: &View,
    width: i32,
    height: i32,
    frame: layout::Frame,
    hovered: Option<usize>,
) -> Result<()> {
    let s = Surface::new(width, height)?;
    draw_panel(
        &s,
        assets,
        view,
        starrupture_wave_core::formatting::now_ms().max(0) as u64,
        frame,
        hovered,
    );
    present(window, &s)
}
fn present(window: HWND, s: &Surface) -> Result<()> {
    let width = s.width;
    let height = s.height;
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT::default();
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    ensure!(
        unsafe {
            UpdateLayeredWindow(
                window,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
                s.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            )
        } != 0,
        "Recipe panel update"
    );
    Ok(())
}
fn close_pixels(hovered: bool) -> Vec<u8> {
    let size = super::close::SIZE as usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let p = &mut pixels[(y * size + x) * 4..][..4];
            // Full click area remains present even around the larger cross.
            p.copy_from_slice(&[0, 0, 0, 1]);
            if (3..size - 3).contains(&x) && (3..size - 3).contains(&y) {
                p.copy_from_slice(&[24, 24, 24, 240]);
            }
            if (6..size - 6).contains(&x)
                && (6..size - 6).contains(&y)
                && ((x as i32 - y as i32).abs() <= 1
                    || ((x + y) as i32 - (size as i32 - 1)).abs() <= 1)
            {
                p.copy_from_slice(if hovered {
                    &[230, 224, 117, 255]
                } else {
                    &[240, 240, 240, 255]
                });
            }
        }
    }
    pixels
}
pub(super) fn close(window: HWND, hovered: bool) -> Result<()> {
    let surface = Surface::new(super::close::SIZE, super::close::SIZE)?;
    let pixels = close_pixels(hovered);
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), surface.bits, pixels.len());
    }
    present(window, &surface)
}
pub(super) fn tooltip(
    window: HWND,
    name: &str,
    font_size: i32,
    max_width: i32,
) -> Result<[i32; 2]> {
    let surface = tooltip_surface(name, font_size, max_width)?;
    present(window, &surface)?;
    Ok([surface.width, surface.height])
}
fn tooltip_surface(name: &str, font_size: i32, max_width: i32) -> Result<Surface> {
    ensure!(!name.is_empty() && name.len() <= 240, "Tooltip name bound");
    let probe = Surface::new(1, 1)?;
    let family = wide("Nippo");
    let font = unsafe {
        CreateFontW(
            -font_size,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            ANTIALIASED_QUALITY as u32,
            0,
            family.as_ptr(),
        )
    };
    ensure!(!font.is_null(), "Tooltip font");
    let value = wide(name);
    let mut measured = RECT {
        left: 0,
        top: 0,
        right: (max_width - 24).max(20),
        bottom: 0,
    };
    unsafe {
        let previous = SelectObject(probe.dc, font);
        DrawTextW(
            probe.dc,
            value.as_ptr(),
            value.len() as i32 - 1,
            &mut measured,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
        );
        SelectObject(probe.dc, previous);
        DeleteObject(font);
    }
    let width = (measured.right + 24).clamp(40, max_width.max(40));
    let height = (measured.bottom + 16).clamp(font_size + 16, 180);
    let surface = Surface::new(width, height)?;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[8, 8, 8, 250]);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), surface.bits, pixels.len());
    }
    Text {
        value: name.into(),
        rect: RECT {
            left: 12,
            top: 8,
            right: width - 12,
            bottom: height - 8,
        },
        size: font_size,
        colour: [240, 240, 235],
        outline: false,
        layout: TextLayout::Centered,
    }
    .draw(surface.dc);
    pixels = surface.pixels();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let p = &mut pixels[(y * width as usize + x) * 4..][..4];
            if x == 0 || y == 0 || x + 1 == width as usize || y + 1 == height as usize {
                p.copy_from_slice(&[218, 222, 220, 255]);
            } else if p[3] == 0 {
                p[3] = 255;
            }
        }
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), surface.bits, pixels.len());
    }
    Ok(surface)
}
fn bitmap(path: &Path, width: i32, height: i32, pixels: Vec<u8>) -> Result<()> {
    let mut header = vec![0u8; 54];
    header[..2].copy_from_slice(b"BM");
    header[2..6].copy_from_slice(&((54 + pixels.len()) as u32).to_le_bytes());
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    header[18..22].copy_from_slice(&width.to_le_bytes());
    header[22..26].copy_from_slice(&(-height).to_le_bytes());
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    header[28..30].copy_from_slice(&32u16.to_le_bytes());
    header.extend(pixels);
    std::fs::write(path, header)?;
    Ok(())
}
fn group_preview(directory: &Path, assets: &Assets, views: &[View]) -> Result<()> {
    let count = views
        .iter()
        .filter_map(|v| v.snapshot.as_ref().map(|s| s.ingredients.len()))
        .max()
        .unwrap();
    for pause in [false, true] {
        let geometry = layout::fit([2560, 1440], views.len(), count, 100, pause)
            .context("Group fixture fit")?;
        let width = (geometry.width + geometry.gap) * views.len() as i32 - geometry.gap;
        let height = geometry.height;
        let mut group = vec![0u8; width as usize * height as usize * 4];
        for (index, view) in views.iter().enumerate() {
            let panel = Surface::new(geometry.width, height)?;
            let frame = layout::frame(index, views.len());
            draw_panel(&panel, assets, view, 1000, frame, None);
            let pixels = panel.pixels();
            let w = geometry.width as usize;
            let h = height as usize;
            let scale = geometry.width as f64 / layout::WIDTH;
            let cut = (10.0 * scale).round().max(5.0) as usize;
            let diagonal_x = w - cut / 2 - 1;
            let diagonal_y = h - (cut - cut / 2) - 1;
            if frame.bottom_cut {
                ensure!(
                    pixels[(diagonal_y * w + diagonal_x) * 4 + 3] == 255
                        && pixels[(w * h - 1) * 4 + 3] == 0,
                    "Rightmost lower cut missing"
                );
            } else {
                ensure!(
                    pixels[(w * h - 1) * 4 + 3] == 255,
                    "Non-rightmost lower corner is not square"
                );
            }
            if frame.top_tab {
                ensure!(
                    pixels[3] == 255 && pixels[(w - 1) * 4 + 3] == 0,
                    "Leftmost top tab missing"
                );
            } else {
                ensure!(
                    pixels[3] == 0 && pixels[(w - 1) * 4 + 3] == 0,
                    "Non-leftmost panel has a top tab"
                );
            }
            ensure!(
                pixels[(cut * w) * 4 + 3] == 255 && pixels[(cut * w + w - 1) * 4 + 3] == 255,
                "Group top baseline does not align"
            );
            let origin = (geometry.width + geometry.gap) * (views.len() - 1 - index) as i32;
            for y in 0..height as usize {
                let from = y * w * 4;
                let to = (y * width as usize + origin as usize) * 4;
                group[to..to + w * 4].copy_from_slice(&pixels[from..from + w * 4]);
            }
            let button = close_pixels(false);
            let size = super::close::SIZE as usize;
            let bx = origin as usize + w - size - 6;
            let by = layout::control_y(geometry.width) as usize;
            ensure!(
                by + size < layout::controls(geometry.width) as usize,
                "Close target overlaps recipe image"
            );
            for y in 0..size {
                for x in 0..size {
                    let s = &button[(y * size + x) * 4..][..4];
                    let d = &mut group[((by + y) * width as usize + bx + x) * 4..][..4];
                    let a = s[3] as u32;
                    for c in 0..3 {
                        d[c] = (s[c] as u32 + d[c] as u32 * (255 - a) / 255).min(255) as u8;
                    }
                    d[3] = (a + d[3] as u32 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
        bitmap(
            &directory.join(format!(
                "{}-panels-{}.bmp",
                views.len(),
                if pause {
                    "pause"
                } else {
                    "normal"
                }
            )),
            width,
            height,
            group,
        )?;
    }
    Ok(())
}
pub fn self_test(directory: &Path) -> Result<()> {
    self_test_at(directory, &super::setup::diagnostic_assets()?)
}
pub(super) fn self_test_at(directory: &Path, asset_directory: &Path) -> Result<()> {
    super::self_test_shortcut()?;
    super::close::self_test()?;
    std::fs::create_dir_all(directory)?;
    let idle = close_pixels(false);
    let highlighted = close_pixels(true);
    ensure!(idle != highlighted, "Close hover appearance is unchanged");
    ensure!(
        idle.as_chunks::<4>()
            .0
            .iter()
            .zip(highlighted.as_chunks::<4>().0.iter())
            .all(|(a, b)| (a[3] > 0) == (b[3] > 0)),
        "Hover changed the clickable footprint"
    );
    bitmap(&directory.join("close-idle.bmp"), super::close::SIZE, super::close::SIZE, idle)?;
    bitmap(
        &directory.join("close-hover.bmp"),
        super::close::SIZE,
        super::close::SIZE,
        highlighted,
    )?;
    let mut assets = Assets::load(asset_directory)?;
    super::items::self_test()?;
    let tip = tooltip_surface("Calcite Sheets", 18, 360)?;
    bitmap(&directory.join("item-name-tooltip.bmp"), tip.width, tip.height, tip.pixels())?;
    for (a, b) in close_pixels(false)
        .as_chunks::<4>()
        .0
        .iter()
        .zip(close_pixels(true).as_chunks::<4>().0.iter())
    {
        if a != b {
            ensure!(
                *a == [240, 240, 240, 255] && *b == [230, 224, 117, 255],
                "Close hover changed more than the cross colour"
            );
        }
    }
    // Exercise every recipe with the actual asset pack, including repeated selection
    // and clear. This also bounds image residency independently of draw output.
    let baseline: usize = assets.images.values().map(|i| i.pixels.capacity()).sum();
    let ids: Vec<_> = assets.catalogue.recipes.keys().cloned().collect();
    let mut maximum = baseline;
    let mut switches = Vec::new();
    for id in &ids {
        let recipe = &assets.catalogue.recipes[id];
        let ingredients = starrupture_observation_model::recipe::progress(
            &recipe.requirements,
            &recipe.requirements.keys().map(|id| (id.clone(), 0)).collect::<Vec<_>>(),
        )
        .map_err(anyhow::Error::msg)?;
        let view = View {
            snapshot: Some(starrupture_observation_model::recipe::Snapshot {
                recipe_id: id.clone(),
                ingredients,
                observed_at_ms: 1000,
                generation: 1,
                process: 1,
            }),
            generation: 1,
            ..View::default()
        };
        let started = std::time::Instant::now();
        assets.prepare(&view)?;
        switches.push(started.elapsed().as_micros());
        let bytes: usize = assets.images.values().map(|i| i.pixels.capacity()).sum();
        maximum = maximum.max(bytes);
        ensure!(bytes <= IMAGE_BUDGET, "Recipe image budget exceeded");
        let addresses: Vec<_> = assets.images.values().map(|i| i.pixels.as_ptr()).collect();
        assets.prepare(&view)?;
        ensure!(
            addresses == assets.images.values().map(|i| i.pixels.as_ptr()).collect::<Vec<_>>(),
            "Repeated selection reloaded pixels"
        );
        assets.clear_images();
        ensure!(
            assets.images.values().map(|i| i.pixels.capacity()).sum::<usize>() == baseline,
            "Cleared recipe retained item images"
        );
    }
    let group_ids = [
        "/Game/Chimera/Crafting/CR_QuantumTranqualizer",
        "/Game/Chimera/Crafting/CR_Airlock",
        "/Game/Chimera/Crafting/CR_RTSC",
    ]
    .map(str::to_owned);
    let group: Vec<_> = group_ids
        .iter()
        .map(|id| {
            let recipe = &assets.catalogue.recipes[id];
            View {
                snapshot: Some(starrupture_observation_model::recipe::Snapshot {
                    recipe_id: id.clone(),
                    ingredients: starrupture_observation_model::recipe::progress(
                        &recipe.requirements,
                        &recipe.requirements.keys().map(|id| (id.clone(), 0)).collect::<Vec<_>>(),
                    )
                    .unwrap(),
                    observed_at_ms: 1000,
                    generation: 1,
                    process: 1,
                }),
                generation: 1,
                ..View::default()
            }
        })
        .collect();
    assets.prepare_views(&group)?;
    for count in 1..=group.len() {
        group_preview(directory, &assets, &group[..count])?;
    }
    for view in &group {
        let surface = Surface::new(
            96,
            layout::controls(96) as i32
                + ((layout::HEADER
                    + layout::ROW * view.snapshot.as_ref().unwrap().ingredients.len() as f64
                    + 4.0)
                    .round()) as i32,
        )?;
        draw(&surface, &assets, view, 1000);
    }
    let addresses: Vec<_> = assets.images.values().map(|i| i.pixels.as_ptr()).collect();
    assets.prepare_views(&group)?;
    ensure!(
        addresses == assets.images.values().map(|i| i.pixels.as_ptr()).collect::<Vec<_>>(),
        "Repeated multi-pin render reloaded icons"
    );
    ensure!(
        assets.images.values().map(|i| i.pixels.capacity()).sum::<usize>() <= IMAGE_BUDGET,
        "Multi-pin image cache exceeded bound"
    );
    assets.prepare_views(&group[1..])?;
    assets.clear_images();
    ensure!(
        assets.images.values().map(|i| i.pixels.capacity()).sum::<usize>() == baseline,
        "Clearing multi-pin cache retained icons"
    );
    switches.sort_unstable();
    std::fs::write(
        directory.join("image-cache-validation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "recipes":ids.len(),"budgetBytes":IMAGE_BUDGET,"idleImageBytes":baseline,"maximumImageBytes":maximum,
            "switchMedianMicros":switches[switches.len()/2],"switchMaximumMicros":switches.last(),
            "repeatedSelectionReusesPixels":true,"clearReleasesItemImages":true
        }))?,
    )?;
    // Verify the private font is actually selected, not silently substituted.
    let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    ensure!(!dc.is_null(), "Font test context");
    let name = wide("Nippo");
    let font = unsafe {
        CreateFontW(
            -20,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            ANTIALIASED_QUALITY as u32,
            0,
            name.as_ptr(),
        )
    };
    ensure!(!font.is_null(), "Recipe test font");
    let old = unsafe { SelectObject(dc, font) };
    let mut face = [0u16; 128];
    let n = unsafe { GetTextFaceW(dc, 128, face.as_mut_ptr()) };
    unsafe {
        SelectObject(dc, old);
        DeleteObject(font);
        DeleteDC(dc);
    }
    ensure!(
        n > 0 && String::from_utf16_lossy(&face[..n as usize]).starts_with("Nippo"),
        "Nippo font was substituted"
    );
    let quantum_order = [
        "/Game/Chimera/Items/I_IonStabilizer",
        "/Game/Chimera/Items/I_Supermagnet",
        "/Game/Chimera/Items/I_PlasticDust",
        "/Game/Chimera/Corporations/I_DataPoint",
    ];
    for (name, width, stale, complete, bio, bag) in [
        ("quantum-partial", 96, false, false, false, 100),
        ("quantum-large", 144, false, false, false, 100),
        ("quantum-pause", 75, false, false, false, 100),
        ("quantum-stale", 96, true, false, false, 100),
        ("quantum-complete", 96, false, true, false, 0),
        ("biofilament-bag", 96, false, false, true, 300),
        ("biofilament-submitted", 96, false, false, true, 0),
        ("biofilament-counter-reference", 96, false, false, true, 0),
    ] {
        let id = if bio {
            "/Game/Chimera/Crafting/CR_Biofilament"
        } else {
            "/Game/Chimera/Crafting/CR_QuantumTranqualizer"
        };
        let recipe = assets.catalogue.recipes.get(id).context("Recipe reference")?;
        let bio_order = [
            "/Game/Chimera/Items/I_Superconductor",
            "/Game/Chimera/Items/I_SyntheticProtein",
            "/Game/Chimera/Items/I_RNAStabilizer",
            "/Game/Chimera/Items/I_SubstanceFHS-135-493",
            "/Game/Chimera/Corporations/I_DataPoint",
        ];
        let order: &[&str] = if bio {
            &bio_order
        } else {
            &quantum_order
        };
        let submitted: Vec<_> = order
            .iter()
            .map(|id| {
                (
                    (*id).into(),
                    if complete || id.ends_with("I_DataPoint") {
                        recipe.requirements[*id]
                    } else if (name == "biofilament-counter-reference"
                        && id.ends_with("I_SubstanceFHS-135-493"))
                        || (bio && bag == 0 && id.ends_with("I_SyntheticProtein"))
                    {
                        300
                    } else if id.ends_with("I_Supermagnet") {
                        100
                    } else {
                        0
                    },
                )
            })
            .collect();
        let mut ingredients =
            starrupture_observation_model::recipe::progress(&recipe.requirements, &submitted)
                .map_err(anyhow::Error::msg)?;
        for item in &mut ingredients {
            item.bag_count = Some(
                if item.item_id.ends_with("I_Supermagnet")
                    || item.item_id.ends_with("I_SyntheticProtein")
                {
                    bag
                } else {
                    0
                },
            );
        }
        let view = View {
            snapshot: Some(starrupture_observation_model::recipe::Snapshot {
                recipe_id: id.into(),
                ingredients,
                observed_at_ms: 1000,
                generation: 1,
                process: 1,
            }),
            message: String::new(),
            generation: 1,
        };
        let height = layout::controls(width) as i32
            + ((layout::HEADER + layout::ROW * order.len() as f64 + 4.0) * width as f64
                / layout::WIDTH)
                .round() as i32;
        assets.prepare(&view)?;
        let surface = Surface::new(width, height)?;
        draw(
            &surface,
            &assets,
            &view,
            if stale {
                7000
            } else {
                1000
            },
        );
        if name == "biofilament-bag" {
            let hover = Surface::new(width, height)?;
            draw_panel(&hover, &assets, &view, 1000, layout::frame(0, 1), Some(1));
            let [x, y, _] = layout::tile(width, height, order.len(), 1);
            let position = ((y + 1) * width + x + 1) as usize * 4;
            ensure!(
                hover.pixels()[position..position + 3] != surface.pixels()[position..position + 3],
                "Material hover did not change the tile colour"
            );
            bitmap(&directory.join("material-hover.bmp"), width, height, hover.pixels())?;
        }
        let pixels = surface.pixels();
        ensure!(
            pixels.len() == width as usize * height as usize * 4 && pixels[3] == 255,
            "Recipe render dimensions/edge"
        );
        ensure!(
            pixels.as_chunks::<4>().0.iter().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]),
            "Recipe alpha not premultiplied"
        );
        let mut header = vec![0u8; 54];
        header[..2].copy_from_slice(b"BM");
        header[2..6].copy_from_slice(&((54 + pixels.len()) as u32).to_le_bytes());
        header[10..14].copy_from_slice(&54u32.to_le_bytes());
        header[14..18].copy_from_slice(&40u32.to_le_bytes());
        header[18..22].copy_from_slice(&width.to_le_bytes());
        header[22..26].copy_from_slice(&(-height).to_le_bytes());
        header[26..28].copy_from_slice(&1u16.to_le_bytes());
        header[28..30].copy_from_slice(&32u16.to_le_bytes());
        header.extend(pixels);
        std::fs::write(directory.join(format!("{name}.bmp")), header)?;
    }
    Ok(())
}
