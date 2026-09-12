//! Visor view on the CPU: the atlas drawn as the panels are mounted,
//! one disc per LED, with the panel's gamma curve applied. This is the
//! same picture the browser harness and the present pass draw, produced
//! without a GPU target so captures and documentation come straight out
//! of the binary.

use crate::layout::Layout;
use crate::layout::atlas::Atlas;
use crate::sinks::Frame;

/// An 8-bit RGB image, row-major.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// Piomatter shows the top `n_planes` bits of a 2.2-gamma 10-bit LUT;
/// re-encoded to sRGB so a monitor shows the panel's light.
pub fn gamma_table(n_planes: u8) -> [u8; 256] {
    let drop = 10 - u32::from(n_planes.clamp(1, 10));
    let mut table = [0u8; 256];
    for (i, out) in table.iter_mut().enumerate() {
        let lut = (i as u32).max((1023.0 * (i as f32 / 255.0).powf(2.2)).round() as u32);
        let truncated = (lut >> drop) << drop;
        let linear = truncated as f32 / 1023.0;
        let srgb = if linear <= 0.003_130_8 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        *out = (srgb.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    table
}

/// Bounds of every panel in viewer space, millimetres: x to the
/// viewer's right, y down, so screen(u, v) = (sigma * face.x, -face.y).
pub fn visor_bounds(layout: &Layout) -> ([f32; 2], [f32; 2]) {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for panel in &layout.panels {
        let t = panel.transform();
        let sigma = panel.side.sigma();
        let (w, h) = (panel.width() as f32, panel.height() as f32);
        for (u, v) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
            let f = t.face_mm(u, v);
            let s = [sigma * f[0], -f[1]];
            min = [min[0].min(s[0]), min[1].min(s[1])];
            max = [max[0].max(s[0]), max[1].max(s[1])];
        }
    }
    (min, max)
}

/// Draw `frame` as the viewer sees the visor, `px_per_mm` pixels per
/// millimetre. The wearer's left side is on the right of the image.
pub fn compose(
    layout: &Layout,
    atlas: &Atlas,
    frame: &Frame,
    px_per_mm: f32,
    apply_gamma: bool,
) -> Image {
    let pad = 6.0 * px_per_mm;
    let (min, max) = visor_bounds(layout);
    let width = ((max[0] - min[0]) * px_per_mm + 2.0 * pad).ceil() as u32;
    let height = ((max[1] - min[1]) * px_per_mm + 2.0 * pad).ceil() as u32;
    let mut image = Image {
        width,
        height,
        rgb: vec![0; (width * height * 3) as usize],
    };
    for px in image.rgb.chunks_exact_mut(3) {
        px.copy_from_slice(&[10, 11, 13]);
    }
    let table = gamma_table(layout.driver.n_planes);
    let stride = atlas.width as usize * Frame::BYTES_PER_PIXEL;
    for (panel, rect) in layout.panels.iter().zip(&atlas.rects) {
        let t = panel.transform();
        let sigma = panel.side.sigma();
        let radius = 0.42 * panel.pitch_mm * px_per_mm;
        for v in 0..panel.height() {
            for u in 0..panel.width() {
                let i =
                    (rect.y + v) as usize * stride + (rect.x + u) as usize * Frame::BYTES_PER_PIXEL;
                let (b, g, r) = (frame.bgra[i], frame.bgra[i + 1], frame.bgra[i + 2]);
                let colour = if apply_gamma {
                    [table[r as usize], table[g as usize], table[b as usize]]
                } else {
                    [r, g, b]
                };
                let f = t.pixel_centre_mm(u, v);
                let cx = (sigma * f[0] - min[0]) * px_per_mm + pad;
                let cy = (-f[1] - min[1]) * px_per_mm + pad;
                disc(&mut image, cx, cy, radius, colour);
            }
        }
    }
    image
}

fn disc(image: &mut Image, cx: f32, cy: f32, radius: f32, colour: [u8; 3]) {
    let r2 = radius * radius;
    let x0 = (cx - radius).floor().max(0.0) as u32;
    let x1 = ((cx + radius).ceil() as u32).min(image.width.saturating_sub(1));
    let y0 = (cy - radius).floor().max(0.0) as u32;
    let y1 = ((cy + radius).ceil() as u32).min(image.height.saturating_sub(1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r2 {
                let i = ((y * image.width + x) * 3) as usize;
                image.rgb[i..i + 3].copy_from_slice(&colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;

    #[test]
    fn the_gamma_table_is_monotonic_and_keeps_the_endpoints() {
        let t = gamma_table(8);
        assert_eq!(t[0], 0, "black stays black");
        assert_eq!(t[255], 255, "white stays white");
        assert!(
            t.windows(2).all(|w| w[0] <= w[1]),
            "table must not decrease"
        );
        assert!(
            t[128] > 128,
            "the identity toe and truncation lift the mid-tones on a monitor"
        );
    }

    #[test]
    fn the_visor_view_places_the_wearers_left_panel_on_the_right_of_the_image() {
        let layout = presets::load("two_64x32").unwrap();
        let atlas = Atlas::build(&layout).unwrap();
        let mut frame = Frame::new(atlas.width, atlas.height);
        // Light the left panel's inner top pixel (atlas 64, 0) green.
        let i = 64 * Frame::BYTES_PER_PIXEL;
        frame.bgra[i..i + 4].copy_from_slice(&[0, 255, 0, 255]);
        let image = compose(&layout, &atlas, &frame, 2.0, false);
        let centre_x = image.width / 2;
        let lit: Vec<(u32, u32)> = (0..image.height)
            .flat_map(|y| (0..image.width).map(move |x| (x, y)))
            .filter(|&(x, y)| image.rgb[((y * image.width + x) * 3 + 1) as usize] == 255)
            .collect();
        assert!(!lit.is_empty(), "the lit LED should appear");
        assert!(
            lit.iter().all(|&(x, _)| x > centre_x),
            "the wearer's left side must be on the viewer's right"
        );
        assert!(
            lit.iter().all(|&(_, y)| y < image.height / 4),
            "an inner top pixel must be near the top"
        );
    }
}
