//! CPU raster: the `strict` tier.
//!
//! Antialiased axis-aligned geometry, straight-alpha compositing, three blend modes and
//! a PNG out. This is the reference renderer the GPU tier is checked against, so it
//! stays deliberately small and obviously correct.

pub use kinema_doc::{Blend, Doc, LayerKind, Rgba};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Straight (non-premultiplied) RGBA8.
    pub data: Vec<u8>,
}

/// Everything needed to set one block of type.
#[derive(Debug, Clone, Copy)]
pub struct TextDraw<'a> {
    pub text: &'a str,
    pub box_x: f64,
    pub box_y: f64,
    pub box_w: f64,
    pub font: &'a str,
    pub weight: u16,
    pub italic: bool,
    pub size: f64,
    pub tracking: f64,
    pub line_height: f64,
    pub align: kinema_doc::Align,
    pub color: Rgba,
    pub alpha: f64,
    pub mode: Blend,
    /// Staggered entrance, if the block has one.
    pub anim: Option<&'a kinema_doc::TextAnim>,
    /// Layer-local seconds, which is the clock the animator reads.
    pub time: f64,
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

    /// Draw a block of lines inside a box with a bundled face, honouring alignment and
    /// the block's own staggered entrance.
    ///
    /// Glyphs are rasterised by the face itself and composited by coverage, so a 96 px
    /// title is a real rendering of the outline rather than an enlarged pixel grid.
    pub fn draw_text_block(&mut self, d: &TextDraw<'_>) {
        let align = match d.align {
            kinema_doc::Align::Left => kinema_text::Align::Left,
            kinema_doc::Align::Center => kinema_text::Align::Center,
            kinema_doc::Align::Right => kinema_text::Align::Right,
        };
        let placed = kinema_text::layout(
            d.text,
            d.font,
            d.weight,
            d.italic,
            d.size as f32,
            d.tracking as f32,
            d.line_height as f32,
            align,
            d.box_x as f32,
            d.box_y as f32,
            d.box_w as f32,
        );
        for p in placed {
            if p.ch.is_whitespace() {
                continue;
            }
            let (anim_alpha, dy) = match d.anim {
                Some(a) => a.at_glyph(d.time, p.char_index, p.word_index, p.line_index),
                None => (1.0, 0.0),
            };
            let alpha = d.alpha * anim_alpha;
            if alpha <= 0.0 {
                continue;
            }
            let (metrics, bitmap) = p.face.rasterize(p.ch, d.size as f32);
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }
            let (gx, gy0) = kinema_text::glyph_origin(&p, &metrics);
            let gy = gy0 + dy as f32;
            for row in 0..metrics.height {
                let y = (gy + row as f32).floor();
                if !(0.0..self.height as f32).contains(&y) {
                    continue;
                }
                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col] as f64 / 255.0;
                    if coverage <= 0.0 {
                        continue;
                    }
                    let x = (gx + col as f32).floor();
                    if !(0.0..self.width as f32).contains(&x) {
                        continue;
                    }
                    self.composite(x as u32, y as u32, d.color, coverage * alpha, d.mode);
                }
            }
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
    /// Straight RGBA8 pixels. The strict tier's contract is these bytes, not the PNG
    /// encoding, so goldens hash this.
    pub pixels: Vec<u8>,
    pub png: Vec<u8>,
    pub layers: Vec<RenderedLayer>,
    pub skipped: Vec<String>,
    /// Subframe samples averaged into this frame; 1 means no motion blur.
    pub samples: u32,
}

/// Draw one frame of one comp on the CPU.
fn draw_at(
    doc: &Doc,
    comp: &str,
    t: f64,
    scale: f64,
) -> Result<(Canvas, Vec<RenderedLayer>, Vec<String>), RenderError> {
    let c = doc
        .comp(comp)
        .map_err(|e| RenderError::Doc(e.to_string()))?;
    let width = ((c.size[0] as f64) * scale).round().max(1.0) as u32;
    let height = ((c.size[1] as f64) * scale).round().max(1.0) as u32;
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
                canvas.draw_text_block(&TextDraw {
                    text: &text.string,
                    box_x: bx,
                    box_y: by,
                    box_w: bw,
                    font: &text.font,
                    weight: text.weight,
                    italic: text.italic,
                    size: text.size * scale,
                    tracking: text.tracking * scale,
                    line_height: text.line_height,
                    align: text.align,
                    color: text.color,
                    alpha,
                    mode: layer.blend,
                    anim: text.anim.as_ref(),
                    time: t - layer.start,
                });
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

    Ok((canvas, drawn, skipped.into_iter().collect()))
}

/// Accumulate in premultiplied colour: averaging straight alpha would darken edges.
fn accumulate(acc: &mut [f32], canvas: &Canvas) {
    for (i, px) in canvas.data.chunks(4).enumerate() {
        let a = px[3] as f32 / 255.0;
        let o = i * 4;
        acc[o] += px[0] as f32 * a;
        acc[o + 1] += px[1] as f32 * a;
        acc[o + 2] += px[2] as f32 * a;
        acc[o + 3] += px[3] as f32;
    }
}

fn average(acc: &[f32], width: u32, height: u32, samples: u32) -> Canvas {
    let k = samples.max(1) as f32;
    let mut canvas = Canvas::new(width, height);
    for i in 0..(width as usize * height as usize) {
        let o = i * 4;
        let alpha = acc[o + 3] / k;
        let a = alpha / 255.0;
        let channel = |v: f32| -> u8 {
            let straight = if a > 0.0 { (v / k) / a } else { 0.0 };
            straight.round().clamp(0.0, 255.0) as u8
        };
        canvas.data[o] = channel(acc[o]);
        canvas.data[o + 1] = channel(acc[o + 1]);
        canvas.data[o + 2] = channel(acc[o + 2]);
        canvas.data[o + 3] = alpha.round().clamp(0.0, 255.0) as u8;
    }
    canvas
}

/// Draw one frame of one comp on the CPU.
///
/// A comp with a shutter angle integrates the exposure: several subframes are drawn inside
/// the shutter window and averaged. That is what stops fast moves from strobing, and it is
/// the same number the `SHUTTER` detector measures in pixels per frame.
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

    let (canvas, drawn, skipped, samples) = if c.shutter > 0.0 {
        let exposure = (c.shutter.clamp(0.0, 360.0) / 360.0) / doc.fps.as_f64();
        let samples = c.samples.clamp(2, 64);
        let mut acc = vec![0f32; (width as usize) * (height as usize) * 4];
        let mut seen: Option<(Vec<RenderedLayer>, Vec<String>)> = None;
        for i in 0..samples {
            let u = i as f64 / (samples - 1) as f64;
            let sub = t - exposure / 2.0 + exposure * u;
            let (cv, layers, skipped) = draw_at(doc, comp, sub, scale)?;
            if seen.is_none() {
                seen = Some((layers, skipped));
            }
            accumulate(&mut acc, &cv);
        }
        let (layers, skipped) = seen.unwrap_or_default();
        (
            average(&acc, width, height, samples),
            layers,
            skipped,
            samples,
        )
    } else {
        let (cv, layers, skipped) = draw_at(doc, comp, t, scale)?;
        (cv, layers, skipped, 1)
    };

    let png = canvas.to_png()?;
    Ok(RenderOutput {
        width,
        height,
        pixels: canvas.data,
        png,
        layers: drawn,
        skipped,
        samples,
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
        let mut canvas = Canvas::new(500, 140);
        canvas.draw_text_block(&TextDraw {
            text: "HELLO",
            box_x: 10.0,
            box_y: 10.0,
            box_w: 480.0,
            font: "Lato",
            weight: 700,
            italic: false,
            size: 70.0,
            tracking: 0.0,
            line_height: 1.2,
            align: kinema_doc::Align::Left,
            color: Rgba::WHITE,
            alpha: 1.0,
            mode: Blend::Normal,
            anim: None,
            time: 0.0,
        });
        let inked = canvas.data.chunks(4).filter(|p| p[3] > 0).count();
        assert!(inked > 2000, "expected letterforms, got {inked} pixels");
        let partial = canvas
            .data
            .chunks(4)
            .filter(|p| p[3] > 0 && p[3] < 255)
            .count();
        assert!(
            partial > 200,
            "a real outline is antialiased, so edges are partial coverage; got {partial}"
        );
    }

    #[test]
    fn a_staggered_reveal_brings_the_letters_in_over_time() {
        // The background is opaque and full-frame, so count pixels brighter than it: the
        // title is the only light thing on the canvas, and a half-faded glyph still counts.
        let ink = |px: &[u8]| px.chunks(4).filter(|p| p[0] > 60).count();
        let mut doc = Doc::starter();
        let anim = kinema_doc::TextAnim {
            kind: kinema_doc::AnimKind::Reveal,
            stagger: 0.1,
            duration: 0.3,
            unit: kinema_doc::AnimUnit::Char,
            ease: None,
            distance: 0.0,
        };
        doc.comps.get_mut("main").unwrap().layers[2]
            .text
            .as_mut()
            .unwrap()
            .anim = Some(anim);
        // Frame 6 is the title's first frame (0.2 s), frame 33 is 1.1 s and well past
        // the last character's entrance.
        let early = render_frame(&doc, "main", 6, 1.0).unwrap();
        let mid = render_frame(&doc, "main", 12, 1.0).unwrap();
        let late = render_frame(&doc, "main", 33, 1.0).unwrap();
        assert!(ink(&early.pixels) < ink(&mid.pixels), "more letters arrive");
        assert!(ink(&mid.pixels) < ink(&late.pixels), "and more after that");
        assert!(
            ink(&early.pixels) < ink(&late.pixels) / 2,
            "frame 6 is early"
        );
    }

    #[test]
    fn two_lines_land_in_two_bands() {
        let mut canvas = Canvas::new(400, 300);
        canvas.draw_text_block(&TextDraw {
            text: "TOP\nBOTTOM",
            box_x: 0.0,
            box_y: 10.0,
            box_w: 400.0,
            font: "Lato",
            weight: 400,
            italic: false,
            size: 60.0,
            tracking: 0.0,
            line_height: 1.2,
            align: kinema_doc::Align::Center,
            color: Rgba::WHITE,
            alpha: 1.0,
            mode: Blend::Normal,
            anim: None,
            time: 0.0,
        });
        let row_has_ink = |y: usize| (0..400).any(|x| canvas.get(x as u32, y as u32).a > 0);
        // Box top is 10; the first baseline sits one ascent below it, so caps start around
        // row 26. The second line is one 1.2 leading (72 px) lower.
        assert!(row_has_ink(45), "first line should be in the top band");
        assert!(row_has_ink(120), "second line should be a leading lower");
        assert!(!row_has_ink(85), "the two lines should not touch");
        assert!(!row_has_ink(240), "the block should not reach the bottom");
    }

    #[test]
    fn alignment_moves_the_line_inside_its_box() {
        let ink_x = |align: kinema_doc::Align| -> i32 {
            let mut canvas = Canvas::new(400, 100);
            canvas.draw_text_block(&TextDraw {
                text: "HI",
                box_x: 0.0,
                box_y: 0.0,
                box_w: 400.0,
                font: "Lato",
                weight: 400,
                italic: false,
                size: 60.0,
                tracking: 0.0,
                line_height: 1.2,
                align,
                color: Rgba::WHITE,
                alpha: 1.0,
                mode: Blend::Normal,
                anim: None,
                time: 0.0,
            });
            (0..400)
                .find(|x| (0..100).any(|y| canvas.get(*x as u32, y).a > 0))
                .unwrap_or(999)
        };
        let left = ink_x(kinema_doc::Align::Left);
        let center = ink_x(kinema_doc::Align::Center);
        let right = ink_x(kinema_doc::Align::Right);
        assert!(left < center && center < right, "{left} {center} {right}");
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
    fn starter_frame_is_byte_identical() {
        // The strict tier promises bit-identical pixels across machines and runs. If this
        // hash moves, the renderer changed: bless it deliberately with
        // KINEMA_BLESS=1 cargo test -p kinema-raster
        let doc = Doc::starter();
        let out = render_frame(&doc, "main", 30, 1.0).unwrap();
        let hash = blake3::hash(&out.pixels).to_hex().to_string();
        const EXPECTED: &str = "d42ffd18561bee9de0e30958a4a1c4491dd8cb68692363eea0ad4f976c9b25de";
        if std::env::var("KINEMA_BLESS").is_ok() {
            panic!("bless: {hash}");
        }
        assert_eq!(hash, EXPECTED, "frame 30 pixels changed");
    }

    #[test]
    fn motion_blur_integrates_the_exposure() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().shutter = 180.0;
        doc.comps.get_mut("main").unwrap().samples = 8;
        let mut no_blur = Doc::starter();
        no_blur.comps.get_mut("main").unwrap().shutter = 0.0;
        let sharp = render_frame(&no_blur, "main", 9, 0.25).unwrap();
        // Frame 9 is mid-move, so the two must differ.
        let blurred = render_frame(&doc, "main", 9, 0.25).unwrap();
        assert_eq!(blurred.samples, 8);
        assert_eq!(sharp.samples, 1);
        assert_ne!(sharp.pixels, blurred.pixels, "a moving title should blur");
        let again = render_frame(&doc, "main", 9, 0.25).unwrap();
        assert_eq!(blurred.pixels, again.pixels, "blur must be deterministic");
    }

    #[test]
    fn a_static_frame_survives_blur() {
        let mut doc = Doc::starter();
        doc.comps.get_mut("main").unwrap().shutter = 180.0;
        let mut no_blur = Doc::starter();
        no_blur.comps.get_mut("main").unwrap().shutter = 0.0;
        // Frame 60 is after the move has settled, so averaging changes nothing.
        let sharp = render_frame(&no_blur, "main", 60, 0.25).unwrap();
        let blurred = render_frame(&doc, "main", 60, 0.25).unwrap();
        let max_diff = sharp
            .pixels
            .iter()
            .zip(blurred.pixels.iter())
            .map(|(a, b)| (*a as i32 - *b as i32).abs())
            .max()
            .unwrap_or(0);
        assert!(max_diff <= 1, "static content shifted by {max_diff}");
    }

    #[test]
    fn the_same_frame_renders_twice_identically() {
        let doc = Doc::starter();
        let a = render_frame(&doc, "main", 45, 1.0).unwrap();
        let b = render_frame(&doc, "main", 45, 1.0).unwrap();
        assert_eq!(a.pixels, b.pixels);
        assert_eq!(a.png, b.png);
    }

    #[test]
    fn scale_changes_output_size() {
        let doc = Doc::starter();
        let out = render_frame(&doc, "main", 0, 0.25).unwrap();
        assert_eq!((out.width, out.height), (480, 270));
    }
}
