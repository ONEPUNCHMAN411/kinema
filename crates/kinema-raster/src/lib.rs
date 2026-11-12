//! CPU raster: the `strict` tier.
//!
//! Antialiased axis-aligned geometry, straight-alpha compositing, three blend modes and
//! a PNG out. This is the reference renderer the GPU tier is checked against, so it
//! stays deliberately small and obviously correct.

pub mod font;

use kinema_doc::{Blend, Doc, LayerKind, Rgba};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Straight (non-premultiplied) RGBA8.
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Canvas {
            width,
            height,
            data: vec![0u8; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn fill(&mut self, color: Rgba) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set(x, y, color);
            }
        }
    }

    fn set(&mut self, x: u32, y: u32, color: Rgba) {
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        self.data[i] = color.r;
        self.data[i + 1] = color.g;
        self.data[i + 2] = color.b;
        self.data[i + 3] = color.a;
    }

    pub fn get(&self, x: u32, y: u32) -> Rgba {
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        Rgba::new(
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        )
    }

    /// Composite one source colour over a pixel with coverage and a blend mode.
    fn composite(&mut self, x: u32, y: u32, src: Rgba, coverage: f64, mode: Blend) {
        if x >= self.width || y >= self.height || coverage <= 0.0 {
            return;
        }
        let src_a = (src.a as f64 / 255.0) * coverage.clamp(0.0, 1.0);
        if src_a <= 0.0 {
            return;
        }
        let dst = self.get(x, y);
        let dst_a = dst.a as f64 / 255.0;

        let blend_channel = |d: u8, s: u8| -> f64 {
            let (d, s) = (d as f64 / 255.0, s as f64 / 255.0);
            match mode {
                Blend::Normal => s,
                Blend::Screen => 1.0 - (1.0 - d) * (1.0 - s),
                Blend::Multiply => d * s,
                Blend::Overlay => {
                    if d <= 0.5 {
                        2.0 * d * s
                    } else {
                        1.0 - 2.0 * (1.0 - d) * (1.0 - s)
                    }
                }
                Blend::Add => (d + s).min(1.0),
            }
        };

        let out = |d: u8, s: u8| -> u8 {
            let blended = blend_channel(d, s);
            let mixed = (d as f64 / 255.0) * (1.0 - src_a) + blended * src_a;
            (mixed * 255.0).round().clamp(0.0, 255.0) as u8
        };

        let out_a = (dst_a + src_a * (1.0 - dst_a)).clamp(0.0, 1.0);
        self.set(
            x,
            y,
            Rgba::new(
                out(dst.r, src.r),
                out(dst.g, src.g),
                out(dst.b, src.b),
                (out_a * 255.0).round() as u8,
            ),
        );
    }

    /// Axis-aligned rectangle with exact area coverage.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: Rgba,
        alpha: f64,
        mode: Blend,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (x0, y0) = (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32);
        let x1 = ((x + w).ceil() as i64).clamp(0, self.width as i64) as u32;
        let y1 = ((y + h).ceil() as i64).clamp(0, self.height as i64) as u32;
        for py in y0..y1 {
            let cy = ((py as f64 + 1.0).min(y + h) - (py as f64).max(y)).max(0.0);
            for px in x0..x1 {
                let cx = ((px as f64 + 1.0).min(x + w) - (px as f64).max(x)).max(0.0);
                let coverage = cx.min(1.0) * cy.min(1.0);
                self.composite(px, py, color, coverage * alpha, mode);
            }
        }
    }

    /// Ellipse inscribed in the rectangle, antialiased from the implicit equation.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_ellipse(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: Rgba,
        alpha: f64,
        mode: Blend,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (rx, ry) = (w / 2.0, h / 2.0);
        let x0 = x.floor().max(0.0) as u32;
        let y0 = y.floor().max(0.0) as u32;
        let x1 = ((x + w).ceil() as i64).clamp(0, self.width as i64) as u32;
        let y1 = ((y + h).ceil() as i64).clamp(0, self.height as i64) as u32;
        let edge = rx.min(ry).max(1.0);
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = (px as f64 + 0.5 - cx) / rx;
                let dy = (py as f64 + 0.5 - cy) / ry;
                let d = (dx * dx + dy * dy).sqrt();
                let coverage = ((1.0 - d) * edge + 0.5).clamp(0.0, 1.0);
                self.composite(px, py, color, coverage * alpha, mode);
            }
        }
    }

    /// Draw with the bitmap face. `y` is the top of the cap box.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        size: f64,
        tracking: f64,
        color: Rgba,
        alpha: f64,
        mode: Blend,
    ) {
        let unit = size / font::GLYPH_H as f64;
        let mut pen = x;
        for ch in text.chars() {
            let glyph = font::rows(ch);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..font::GLYPH_W {
                    if bits & (1 << (font::GLYPH_W - 1 - col)) != 0 {
                        self.fill_rect(
                            pen + col as f64 * unit,
                            y + row as f64 * unit,
                            unit,
                            unit,
                            color,
                            alpha,
                            mode,
                        );
                    }
                }
            }
            pen += kinema_doc::Text::ADVANCE * unit + tracking;
        }
    }

    pub fn to_png(&self) -> Result<Vec<u8>, RenderError> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| RenderError::Encode(e.to_string()))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| RenderError::Encode(e.to_string()))?;
        }
        Ok(buf)
    }
}

#[derive(Debug)]
pub enum RenderError {
    Unsupported { kind: String, layer: String },
    Encode(String),
    Doc(String),
}

impl RenderError {
    pub fn to_json(&self) -> Value {
        match self {
            RenderError::Unsupported { kind, layer } => json!({
                "code": "not-implemented",
                "message": format!("the CPU tier cannot draw `{kind}` layers yet"),
                "hint": "P0 draws solid, shape and text layers; media, precomps and adjustment layers land in P1",
                "fix_op": { "op": "delete-layer", "comp": "main", "layer": layer, "reason": "hide an unsupported layer to unblock this render" },
                "doc": "docs/PLAN.md#plan",
                "layer": layer,
                "kind": kind,
            }),
            RenderError::Encode(msg) => json!({
                "code": "encode-failed",
                "message": msg,
                "hint": "check the output path is writable",
                "fix_op": null,
                "doc": "docs/PLAN.md#build",
            }),
            RenderError::Doc(msg) => json!({
                "code": "invalid-doc",
                "message": msg,
                "hint": "run `kin check` to see structural problems",
                "fix_op": null,
                "doc": "docs/PLAN.md#document",
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderedLayer {
    pub id: String,
    pub kind: &'static str,
    pub opacity: f64,
}

#[derive(Debug)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    pub layers: Vec<RenderedLayer>,
    pub skipped: Vec<String>,
}

/// Draw one frame of one comp on the CPU.
pub fn render_frame(
    doc: &Doc,
    comp: &str,
    frame: u32,
    scale: f64,
) -> Result<RenderOutput, RenderError> {
    let c = doc
        .comp(comp)
        .map_err(|e| RenderError::Doc(e.to_string()))?;
    let width = ((c.size[0] as f64) * scale).round().max(1.0) as u32;
    let height = ((c.size[1] as f64) * scale).round().max(1.0) as u32;
    let t = doc.frame_time(frame);
    let mut canvas = Canvas::new(width, height);
    let mut drawn = Vec::new();
    let mut skipped = BTreeSet::new();

    for layer in &c.layers {
        if !(t >= layer.start && t < layer.start + layer.duration) {
            continue;
        }
        let ev = doc
            .eval_layer(comp, layer, t)
            .map_err(|e| RenderError::Doc(e.to_string()))?;
        let alpha = ev.opacity;
        let [bx, by, bw, bh] = ev.bbox;
        let (bx, by, bw, bh) = (bx * scale, by * scale, bw * scale, bh * scale);
        match layer.kind {
            LayerKind::Solid => {
                let color = layer.solid.as_ref().map(|s| s.color).unwrap_or(Rgba::BLACK);
                canvas.fill_rect(
                    0.0,
                    0.0,
                    canvas.width as f64,
                    canvas.height as f64,
                    color,
                    alpha,
                    layer.blend,
                );
            }
            LayerKind::Shape => {
                let Some(shape) = &layer.shape else {
                    skipped.insert(layer.id.clone());
                    continue;
                };
                if shape.kind == kinema_doc::ShapeKind::Ellipse {
                    canvas.fill_ellipse(bx, by, bw, bh, shape.color, alpha, layer.blend);
                } else {
                    canvas.fill_rect(bx, by, bw, bh, shape.color, alpha, layer.blend);
                }
            }
            LayerKind::Text => {
                let Some(text) = &layer.text else {
                    skipped.insert(layer.id.clone());
                    continue;
                };
                let color = text.color;
                canvas.draw_text(
                    &text.string,
                    bx,
                    by,
                    text.size * scale,
                    text.tracking * scale,
                    color,
                    alpha,
                    layer.blend,
                );
            }
            other => {
                return Err(RenderError::Unsupported {
                    kind: other.as_str().to_string(),
                    layer: layer.id.clone(),
                })
            }
        }
        drawn.push(RenderedLayer {
            id: layer.id.clone(),
            kind: layer.kind.as_str(),
            opacity: alpha,
        });
    }

    Ok(RenderOutput {
        width,
        height,
        png: canvas.to_png()?,
        layers: drawn,
        skipped: skipped.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinema_doc::Doc;

    #[test]
    fn a_frame_renders_and_encodes() {
        let doc = Doc::starter();
        // Frame 30 is past the staggered entrances, so all three layers are on screen.
        let out = render_frame(&doc, "main", 30, 1.0).unwrap();
        assert_eq!((out.width, out.height), (1920, 1080));
        assert!(out.png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert_eq!(out.layers.len(), 3);
        assert!(out.skipped.is_empty());
    }

    #[test]
    fn background_pixel_is_the_solid_colour() {
        let doc = Doc::starter();
        let c = doc.comp("main").unwrap();
        let mut canvas = Canvas::new(64, 64);
        let solid = c.layers[0].solid.as_ref().unwrap();
        canvas.fill_rect(0.0, 0.0, 64.0, 64.0, solid.color, 1.0, Blend::Normal);
        assert_eq!(canvas.get(10, 10), solid.color);
    }

    #[test]
    fn text_puts_ink_on_the_canvas() {
        let mut canvas = Canvas::new(400, 120);
        canvas.draw_text(
            "HELLO",
            10.0,
            10.0,
            70.0,
            0.0,
            Rgba::WHITE,
            1.0,
            Blend::Normal,
        );
        let inked = canvas.data.chunks(4).filter(|p| p[3] > 0).count();
        assert!(inked > 500, "expected letterforms, got {inked} pixels");
    }

    #[test]
    fn opacity_and_screen_blend_are_applied() {
        let mut canvas = Canvas::new(4, 4);
        canvas.fill_rect(
            0.0,
            0.0,
            4.0,
            4.0,
            Rgba::opaque(100, 100, 100),
            1.0,
            Blend::Normal,
        );
        canvas.fill_rect(
            0.0,
            0.0,
            4.0,
            4.0,
            Rgba::opaque(100, 100, 100),
            1.0,
            Blend::Screen,
        );
        let px = canvas.get(1, 1);
        assert!(px.r > 100, "screen should lighten, got {px}");
        let mut half = Canvas::new(4, 4);
        half.fill_rect(0.0, 0.0, 4.0, 4.0, Rgba::WHITE, 0.5, Blend::Normal);
        let px = half.get(1, 1);
        assert!(
            (px.r as i32 - 128).abs() <= 1,
            "50% white over black, got {px}"
        );
    }

    #[test]
    fn unsupported_layers_fail_loudly() {
        let mut doc = Doc::starter();
        let mut layer = doc.comps["main"].layers[0].clone();
        layer.id = "img".into();
        layer.kind = LayerKind::Image;
        doc.comps.get_mut("main").unwrap().layers.push(layer);
        let err = render_frame(&doc, "main", 0, 1.0).unwrap_err();
        assert_eq!(err.to_json()["code"], "not-implemented");
    }

    #[test]
    fn scale_changes_output_size() {
        let doc = Doc::starter();
        let out = render_frame(&doc, "main", 0, 0.25).unwrap();
        assert_eq!((out.width, out.height), (480, 270));
    }
}
