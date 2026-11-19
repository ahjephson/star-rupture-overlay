use anyhow::{Context, Result, ensure};
use serde_json::Value;
pub(super) fn decode(
    reader: &crate::properties::Reader<'_>,
    meta: &Value,
    props: &Value,
) -> Result<Vec<u8>> {
    let n = |k: &str| -> Result<usize> {
        usize::try_from(meta[k].as_u64().context("Icon metadata integer")?).map_err(Into::into)
    };
    let (w, h, offset, size) = (n("width")?, n("height")?, n("offset")?, n("size")?);
    ensure!((1..=1024).contains(&w) && (1..=1024).contains(&h), "Icon dimensions");
    let data = reader
        .bytes
        .get(offset..offset.checked_add(size).context("Icon range overflow")?)
        .context("Icon payload")?;
    let mut pixels = vec![0; w * h * 4];
    match meta["format"].as_str() {
        Some("PF_B8G8R8A8") => {
            ensure!(data.len() == pixels.len(), "Raw icon shape");
            pixels.copy_from_slice(data)
        }
        Some("PF_DXT5") => {
            ensure!(
                w.is_multiple_of(4) && h.is_multiple_of(4) && data.len() == w * h,
                "BC3 icon shape"
            );
            for by in 0..h / 4 {
                for bx in 0..w / 4 {
                    let start = (by * (w / 4) + bx) * 16;
                    let mut block = [0; 64];
                    bcdec_rs::bc3(&data[start..start + 16], &mut block, 16);
                    for y in 0..4 {
                        for x in 0..4 {
                            let s = (y * 4 + x) * 4;
                            let d = ((by * 4 + y) * w + bx * 4 + x) * 4;
                            pixels[d..d + 4].copy_from_slice(&[
                                block[s + 2],
                                block[s + 1],
                                block[s],
                                block[s + 3],
                            ]);
                        }
                    }
                }
            }
        }
        Some("PF_FloatRGBA") => {
            ensure!(props["SRGB"] == false && data.len() == w * h * 8, "Linear float icon shape");
            for (input, output) in data.as_chunks::<8>().0.iter().zip(pixels.as_chunks_mut::<4>().0)
            {
                let mut c = [0.0; 4];
                for (i, b) in input.as_chunks::<2>().0.iter().enumerate() {
                    c[i] = half(u16::from_le_bytes(*b))?;
                }
                output.copy_from_slice(&[
                    color(c[2]),
                    color(c[1]),
                    color(c[0]),
                    (c[3].clamp(0.0, 1.0) * 255.0).round() as u8,
                ]);
            }
        }
        _ => anyhow::bail!("Unsupported catalogue icon format"),
    }
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    let mut out = Vec::with_capacity(8 + pixels.len());
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    out.extend_from_slice(&pixels);
    Ok(out)
}
fn half(bits: u16) -> Result<f32> {
    let e = i32::from((bits >> 10) & 31);
    ensure!(e != 31, "Nonfinite float icon component");
    let f = f32::from(bits & 1023) / 1024.0;
    let v = if e == 0 {
        f * 2.0f32.powi(-14)
    } else {
        (1.0 + f) * 2.0f32.powi(e - 15)
    };
    Ok(if bits & 0x8000 == 0 {
        v
    } else {
        -v
    })
}
fn color(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let v = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_float_color_is_finite_and_preserves_linear_alpha() {
        assert_eq!(half(0x3800).unwrap(), 0.5);
        assert!(half(0x7c00).is_err());
        assert!(half(0x7e00).is_err());
        assert_eq!(color(0.5), 188);
        assert_eq!(color(-1.0), 0);
    }
}
