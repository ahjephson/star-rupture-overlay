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
                if !self.outline || longest <= self.rect.right - self.rect.left - 4 || size == 8 {
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
    let scale = width as f64 / 120.0;
    let row = (((height as f64 - 155.0 * scale) / count.max(1) as f64).min(98.0 * scale)).max(20.0);
    let tile = (80.0 * scale).min(row - 8.0 * scale).round() as i32;
    let x = (width - tile) / 2;
    let cat = &assets.catalogue;
    let fresh = view
        .snapshot
        .as_ref()
        .is_some_and(|s| s.observed_at_ms <= now && now - s.observed_at_ms <= 5000)
        && view.message.is_empty();
    if let Some(snapshot) = &view.snapshot {
        if let Some(recipe) = cat.recipes.get(&snapshot.recipe_id) {
            let header = (78.0 * scale) as i32;
            let hx = (width - header) / 2;
            let hy = (20.0 * scale) as i32;
            let frame_rect = [hx, hy, header, (92.0 * scale).round() as i32];
            image_rect(&mut pixels, [w, h], &assets.header_background, frame_rect, None);
            image(
                &mut pixels,
                [w, h],
                &assets.images[&cat.items[&recipe.item_id].image],
                hx + (2.0 * scale) as i32,
                hy + (2.0 * scale) as i32,
                (74.0 * scale) as i32,
                None,
            );
            image_rect(&mut pixels, [w, h], &assets.header_frame, frame_rect, None);
            texts.push(Text {
                value: recipe.name.replacen(' ', "\n", 1),
                rect: RECT {
                    left: 8,
                    top: (118.0 * scale) as i32,
                    right: width - 8,
                    bottom: (151.0 * scale) as i32,
                },
                size: (14.0 * scale) as i32,
                colour: [240, 240, 235],
                outline: false,
                layout: TextLayout::Centered,
            });
            for (index, item) in snapshot.ingredients.iter().enumerate() {
                let y = (155.0 * scale + index as f64 * row) as i32;
                let complete = fresh && item.remaining == 0;
                let bg = if complete {
                    [55, 55, 55, 255]
                } else {
                    [107, 119, 133, 255]
                };
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
                                    56.0
                                } else {
                                    28.0
                                } * scale) as i32,
                            right: x + tile,
                            bottom: y + tile,
                        },
                        size: (19.0 * scale).max(10.0) as i32,
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
                            line_advance: (28.0 * scale).round() as i32,
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
            if (x > w / 3 && y < (x - w / 3).min(cut)) || x + y > w + h - 2 - cut {
                p.fill(0);
                continue;
            }
            if x == 0
                || x == w - 1
                || y == h - 1
                || (y == 0 && x <= w / 3)
                || (x > w / 3 && y == (x - w / 3).min(cut))
            {
                p.copy_from_slice(&[218, 222, 220, 255]);
            } else if p[3] == 0 {
                p[3] = 255
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
) -> Result<()> {
    let s = Surface::new(width, height)?;
    draw(&s, assets, view, starrupture_wave_core::formatting::now_ms().max(0) as u64);
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
pub fn self_test(directory: &Path) -> Result<()> {
    super::self_test_shortcut()?;
    std::fs::create_dir_all(directory)?;
    let exe = std::env::current_exe()?;
    let assets = Assets::load(&exe.parent().context("Executable folder")?.join("recipe-assets"))?;
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
        ("quantum-partial", 120, false, false, false, 100),
        ("quantum-large", 180, false, false, false, 100),
        ("quantum-stale", 120, true, false, false, 100),
        ("quantum-complete", 120, false, true, false, 0),
        ("biofilament-bag", 120, false, false, true, 300),
        ("biofilament-submitted", 120, false, false, true, 0),
        ("biofilament-counter-reference", 120, false, false, true, 0),
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
        let height = (if bio {
            645
        } else {
            547
        }) * width
            / 120;
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
