//! CPU raster: the `strict` tier.
//!
//! Antialiased axis-aligned geometry, straight-alpha compositing, three blend modes and
//! a PNG out. This is the reference renderer the GPU tier is checked against, so it
//! stays deliberately small and obviously correct.

pub use kinema_doc::{Blend, Doc, LayerKind, Rgba};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, OnceLock};

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

        // An opaque normal-blend rect that covers the canvas is a memcpy: the coverage
        // maths below would compute exactly this colour for every pixel, two million times.
        if matches!(mode, Blend::Normal)
            && alpha >= 1.0
            && color.a == 255
            && x0 == 0
            && y0 == 0
            && x1 >= self.width
            && y1 >= self.height
        {
            let px = [color.r, color.g, color.b, 255u8];
            for out in self.data.as_chunks_mut::<4>().0 {
                *out = px;
            }
            return;
        }

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

    /// Keep a canvas only where `matte` has coverage.
    ///
    /// An alpha matte reads the matte layer's alpha; a luma matte reads its brightness
    /// (Rec. 709 on the encoded values, which is what a luma matte means to a compositor)
    /// and still respects the matte layer's own alpha.
    pub fn mask_by(&mut self, matte: &Canvas, luma: bool) {
        for (i, px) in self.data.chunks_mut(4).enumerate() {
            let m = &matte.data[i * 4..i * 4 + 4];
            let a = m[3] as f64 / 255.0;
            let cover = if luma {
                (0.2126 * m[0] as f64 + 0.7152 * m[1] as f64 + 0.0722 * m[2] as f64) / 255.0 * a
            } else {
                a
            };
            px[3] = (px[3] as f64 * cover).round().clamp(0.0, 255.0) as u8;
        }
    }

    /// Composite `src` onto this canvas with its top-left at (dx, dy), in canvas pixels.
    pub fn composite_at(&mut self, src: &Canvas, dx: f64, dy: f64, alpha: f64, mode: Blend) {
        let ox = dx.round() as i64;
        let oy = dy.round() as i64;
        for sy in 0..src.height {
            let ty = oy + sy as i64;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            for sx in 0..src.width {
                let tx = ox + sx as i64;
                if tx < 0 || tx >= self.width as i64 {
                    continue;
                }
                let s = src.get(sx, sy);
                if s.a == 0 {
                    continue;
                }
                self.composite(tx as u32, ty as u32, s, alpha, mode);
            }
        }
    }

    /// Composite another canvas of the same size at full coverage.
    pub fn composite_from(&mut self, src: &Canvas, mode: Blend) {
        let w = self.width.min(src.width);
        let h = self.height.min(src.height);
        for py in 0..h {
            for px in 0..w {
                let s = src.get(px, py);
                if s.a == 0 {
                    continue;
                }
                self.composite(px, py, s, 1.0, mode);
            }
        }
    }

    /// A rectangle filled with a ramp.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_gradient(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        g: &kinema_doc::Gradient,
        alpha: f64,
        mode: Blend,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let x0 = x.floor().max(0.0) as u32;
        let y0 = y.floor().max(0.0) as u32;
        let x1 = ((x + w).ceil() as i64).clamp(0, self.width as i64) as u32;
        let y1 = ((y + h).ceil() as i64).clamp(0, self.height as i64) as u32;
        let proj = g.projector(x, y, w, h);

        // An opaque normal-blend ramp over the whole canvas writes straight into the
        // buffer: coverage is 1 everywhere, so compositing would only multiply by one.
        //
        // The ramp is tabulated first because the sRGB transfer is a `powf`: evaluated per
        // pixel per subframe it dominates the frame. 2048 entries across the whole ramp is
        // finer than 8-bit output can show, so the table costs nothing visually.
        if matches!(mode, Blend::Normal)
            && alpha >= 1.0
            && x0 == 0
            && y0 == 0
            && x1 >= self.width
            && y1 >= self.height
        {
            // Motion blur asks for the same ramp once per subframe and a sequence asks for
            // it once per frame, so the tabulated result is kept: the second and later
            // requests are a memcpy.
            let key = ramp_key(g, self.width, self.height);
            let cached = ramp_cache()
                .lock()
                .ok()
                .and_then(|c| c.iter().find(|e| e.key == key).map(|e| e.pixels.clone()));
            let pixels = match cached {
                Some(p) => p,
                None => {
                    let built = Arc::new(build_ramp(g, &proj, self.width, self.height));
                    if let Ok(mut c) = ramp_cache().lock() {
                        if c.len() >= 2 {
                            c.clear();
                        }
                        c.push(RampEntry {
                            key,
                            pixels: built.clone(),
                        });
                    }
                    built
                }
            };
            self.data.copy_from_slice(&pixels);
            return;
        }

        for py in y0..y1 {
            let cy = ((py as f64 + 1.0).min(y + h) - (py as f64).max(y)).max(0.0);
            for px in x0..x1 {
                let cx = ((px as f64 + 1.0).min(x + w) - (px as f64).max(x)).max(0.0);
                let coverage = cx.min(1.0) * cy.min(1.0);
                let t = proj.at(px as f64 + 0.5, py as f64 + 0.5);
                self.composite(px, py, shaded(g, t, px, py), coverage * alpha, mode);
            }
        }
    }

    /// An ellipse filled with a ramp, radially masked like `fill_ellipse`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_ellipse_gradient(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        g: &kinema_doc::Gradient,
        alpha: f64,
        mode: Blend,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (rx, ry) = (w / 2.0, h / 2.0);
        let (cx, cy) = (x + rx, y + ry);
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
                if coverage <= 0.0 {
                    continue;
                }
                let t = g.project(px as f64 + 0.5, py as f64 + 0.5, x, y, w, h);
                self.composite(px, py, shaded(g, t, px, py), coverage * alpha, mode);
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
            // Glyphs repeat across subframes and across frames of a sequence, so this
            // comes from the face's cache rather than re-rasterising the outline.
            let glyph = p.face.glyph(p.ch, d.size as f32);
            let (metrics, bitmap) = (glyph.0, glyph.1.as_slice());
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

/// A 4x4 ordered dither matrix, in sixteenths.
const BAYER4: [[f64; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

/// The ramp colour at `t`, with the ordered-dither bias applied when the gradient asks for
/// it. Dithering trades a visible step for invisible noise, which is the correct trade at 8
/// bits per channel.
fn quantise(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

struct RampEntry {
    key: String,
    pixels: Arc<Vec<u8>>,
}

fn ramp_cache() -> &'static Mutex<Vec<RampEntry>> {
    static CACHE: OnceLock<Mutex<Vec<RampEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn ramp_key(g: &kinema_doc::Gradient, w: u32, h: u32) -> String {
    format!("{w}x{h}|{}", serde_json::to_string(g).unwrap_or_default())
}

/// Tabulate a whole-canvas ramp into RGBA bytes.
///
/// The sRGB transfer is a `powf`, so the ramp is sampled at 2048 steps first — finer than
/// 8-bit output can show — and then only looked up per pixel.
fn build_ramp(g: &kinema_doc::Gradient, proj: &kinema_doc::Projector, w: u32, h: u32) -> Vec<u8> {
    const LUT: usize = 2048;
    let table: Vec<[f64; 3]> = (0..LUT)
        .map(|i| {
            let lin = g.sample_linear(i as f64 / (LUT - 1) as f64);
            [
                Rgba::srgb_from_linear(lin[0]),
                Rgba::srgb_from_linear(lin[1]),
                Rgba::srgb_from_linear(lin[2]),
            ]
        })
        .collect();
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for py in 0..h {
        for px in 0..w {
            let t = proj.at(px as f64 + 0.5, py as f64 + 0.5);
            let idx = ((t * (LUT - 1) as f64).round() as usize).min(LUT - 1);
            let entry = table[idx];
            let bias = if g.dither {
                BAYER4[(py % 4) as usize][(px % 4) as usize] / 16.0 - 0.5
            } else {
                0.0
            };
            let o = ((py * w + px) * 4) as usize;
            out[o] = quantise(entry[0] * 255.0 + bias);
            out[o + 1] = quantise(entry[1] * 255.0 + bias);
            out[o + 2] = quantise(entry[2] * 255.0 + bias);
            out[o + 3] = 255;
        }
    }
    out
}

fn shaded(g: &kinema_doc::Gradient, t: f64, px: u32, py: u32) -> Rgba {
    let lin = g.sample_linear(t);
    // The bias has to land before rounding, on the unquantised sRGB value: adding it to an
    // 8-bit number can only ever return that same number.
    let bias = if g.dither {
        BAYER4[(py % 4) as usize][(px % 4) as usize] / 16.0 - 0.5
    } else {
        0.0
    };
    let ch = |v: f64| {
        (kinema_doc::Rgba::srgb_from_linear(v) * 255.0 + bias)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba::opaque(ch(lin[0]), ch(lin[1]), ch(lin[2]))
}

#[derive(Debug)]
pub enum RenderError {
    Unsupported {
        kind: String,
        layer: String,
    },
    Encode(String),
    Doc(String),
    Matte {
        layer: String,
        source: String,
        why: String,
    },
    Comp {
        layer: String,
        source: String,
        why: String,
        fix_op: Option<Value>,
    },
    Modifier {
        layer: String,
        kind: String,
    },
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
            RenderError::Modifier { layer, kind } => json!({
                "code": "modifier",
                "message": format!("the CPU tier does not know the `{kind}` modifier"),
                "hint": "this build applies `glow`",
                "fix_op": null,
                "doc": "docs/PLAN.md#layers",
                "layer": layer,
                "kind": kind,
            }),
            RenderError::Comp {
                layer,
                source,
                why,
                fix_op,
            } => json!({
                "code": "comp",
                "message": why,
                "hint": "a comp layer names another comp in `source`; nested comps nest one level at a time and none may contain itself",
                "fix_op": fix_op,
                "doc": "docs/PLAN.md#layers",
                "layer": layer,
                "source": source,
            }),
            RenderError::Matte { layer, source, why } => json!({
                "code": "matte",
                "message": why,
                "hint": "a matte names another layer in the same comp, by id",
                "fix_op": { "op": "set-mattes", "comp": null, "layer": layer, "mattes": null, "reason": "drop the matte that cannot be resolved" },
                "doc": "docs/PLAN.md#layers",
                "layer": layer,
                "source": source,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderedLayer {
    pub id: String,
    pub kind: &'static str,
    pub opacity: f64,
    pub matte: Option<String>,
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
    /// Layers that were used as someone's matte, and so did not draw themselves. Reported
    /// because a layer vanishing quietly is the failure mode mattes are prone to.
    pub matte_sources: Vec<String>,
    /// Subframe samples averaged into this frame; 1 means no motion blur.
    pub samples: u32,
}

/// Draw one frame of one comp on the CPU.
fn draw_at(
    doc: &Doc,
    comp: &str,
    t: f64,
    scale: f64,
    stack: &[String],
    depth: usize,
) -> Result<(Canvas, Vec<RenderedLayer>, Vec<String>), RenderError> {
    if depth > 8 {
        return Err(RenderError::Comp {
            layer: String::new(),
            source: comp.to_string(),
            why: format!("comps nest more than 8 deep at `{comp}`"),
            fix_op: None,
        });
    }
    let c = doc
        .comp(comp)
        .map_err(|e| RenderError::Doc(e.to_string()))?;
    let width = ((c.size[0] as f64) * scale).round().max(1.0) as u32;
    let height = ((c.size[1] as f64) * scale).round().max(1.0) as u32;
    let mut canvas = Canvas::new(width, height);
    let mut drawn = Vec::new();
    let mut skipped = BTreeSet::new();

    // A layer named as someone's matte is a source, not a picture: it is composed into the
    // layer that refers to it instead of drawing itself.
    let sources = matte_sources(c);

    for layer in &c.layers {
        if !(t >= layer.start && t < layer.start + layer.duration) {
            continue;
        }
        if sources.contains(&layer.id) {
            continue;
        }
        let ev = doc
            .eval_layer(comp, layer, t)
            .map_err(|e| RenderError::Doc(e.to_string()))?;
        let alpha = ev.opacity;
        let matte = layer.mattes.as_ref();
        let offscreen = matte.is_some() || !layer.modifiers.is_empty();

        if !offscreen {
            if !draw_layer(
                &mut canvas,
                doc,
                comp,
                layer,
                &ev,
                scale,
                t,
                layer.blend,
                stack,
                depth,
            )? {
                skipped.insert(layer.id.clone());
                continue;
            }
            drawn.push(RenderedLayer {
                id: layer.id.clone(),
                kind: layer.kind.as_str(),
                opacity: alpha,
                matte: None,
            });
            continue;
        }

        // A matte is built in canvas space, so the matte layer's own transform decides
        // where it cuts: that is the whole point of using one. Modifiers need the layer
        // on its own canvas for a different reason: a glow is made from its coverage.
        let mut own = Canvas::new(width, height);
        if !draw_layer(
            &mut own,
            doc,
            comp,
            layer,
            &ev,
            scale,
            t,
            Blend::Normal,
            stack,
            depth,
        )? {
            skipped.insert(layer.id.clone());
            continue;
        }

        // Glow goes under the layer, onto the frame, so the layer itself stays sharp.
        let [bx, by, bw, bh] = ev.bbox;
        let bbox = [bx * scale, by * scale, bw * scale, bh * scale];
        for m in &layer.modifiers {
            apply_modifier(&mut canvas, &mut own, layer, bbox, m, scale)?;
        }

        let mut matte_desc = None;
        if let Some(m) = matte {
            let (source, luma) = match (&m.luma, &m.alpha) {
                (Some(id), _) => (id, true),
                (None, Some(id)) => (id, false),
                (None, None) => {
                    return Err(RenderError::Matte {
                        layer: layer.id.clone(),
                        source: String::new(),
                        why: format!("layer `{}` declares a matte with no source", layer.id),
                    })
                }
            };
            let Some(src) = c.layers.iter().find(|l| &l.id == source) else {
                return Err(RenderError::Matte {
                    layer: layer.id.clone(),
                    source: source.clone(),
                    why: format!(
                        "layer `{}` mattes `{source}`, which is not in comp `{comp}`",
                        layer.id
                    ),
                });
            };
            let mut mask = Canvas::new(width, height);
            if t >= src.start && t < src.start + src.duration {
                let sev = doc
                    .eval_layer(comp, src, t)
                    .map_err(|e| RenderError::Doc(e.to_string()))?;
                draw_layer(
                    &mut mask,
                    doc,
                    comp,
                    src,
                    &sev,
                    scale,
                    t,
                    Blend::Normal,
                    stack,
                    depth,
                )?;
            }
            own.mask_by(&mask, luma);
            matte_desc = Some(format!(
                "{} through `{source}`",
                if luma { "luma" } else { "alpha" }
            ));
        }
        canvas.composite_from(&own, layer.blend);
        drawn.push(RenderedLayer {
            id: layer.id.clone(),
            kind: layer.kind.as_str(),
            opacity: alpha,
            matte: matte_desc,
        });
    }

    Ok((canvas, drawn, skipped.into_iter().collect()))
}

/// Apply one modifier to the frame, reading the layer's own coverage as the source.
///
/// An unknown type is an error with the layer named: silently ignoring a modifier is
/// exactly how a document ends up not looking like what it says.
fn apply_modifier(
    canvas: &mut Canvas,
    own: &mut Canvas,
    layer: &kinema_doc::Layer,
    bbox: [f64; 4],
    m: &kinema_doc::Modifier,
    scale: f64,
) -> Result<(), RenderError> {
    let num = |key: &str, default: f64| -> f64 {
        m.params
            .get(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    };
    // Only the layer's box grown by the radius can be touched. Working over the whole
    // canvas made a 40 px glow cost more than the rest of the frame.
    let [bx, by, bw, bh] = bbox;
    let grow = |pad: f64, w: u32, h: u32| -> [u32; 4] {
        [
            (bx - pad).floor().max(0.0) as u32,
            (by - pad).floor().max(0.0) as u32,
            ((bx + bw + pad).ceil() as i64).clamp(0, w as i64) as u32,
            ((by + bh + pad).ceil() as i64).clamp(0, h as i64) as u32,
        ]
    };
    match m.kind.as_str() {
        "glow" => {
            let color = m
                .params
                .get("color")
                .and_then(|v| serde_json::from_value::<Rgba>(v.clone()).ok())
                .unwrap_or(Rgba::opaque(255, 255, 255));
            let pad = num("radius", 24.0) * scale;
            let rect = grow(pad, canvas.width, canvas.height);
            glow(
                canvas,
                own,
                rect,
                num("radius", 24.0) * scale,
                num("strength", 0.5),
                color,
            );
            Ok(())
        }
        "blur" => {
            let radius = num("radius", 12.0) * scale;
            let rect = grow(radius, own.width, own.height);
            blur_region(own, rect, radius);
            Ok(())
        }
        other => Err(RenderError::Modifier {
            layer: layer.id.clone(),
            kind: other.to_string(),
        }),
    }
}

/// Blur the layer in place over `rect`.
///
/// Colour is premultiplied before the blur and divided out afterwards, or a soft edge
/// picks up the colour of the transparent pixels beside it.
fn blur_region(own: &mut Canvas, rect: [u32; 4], radius: f64) {
    let [x0, y0, x1, y1] = rect;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let r = (radius.max(1.0) / 3.0).round().max(1.0) as usize;
    let mut planes: Vec<Vec<f32>> = (0..4).map(|_| Vec::with_capacity(w * h)).collect();
    for y in 0..h {
        for x in 0..w {
            let p = own.get(x0 + x as u32, y0 + y as u32);
            let a = p.a as f32 / 255.0;
            planes[0].push(p.r as f32 / 255.0 * a);
            planes[1].push(p.g as f32 / 255.0 * a);
            planes[2].push(p.b as f32 / 255.0 * a);
            planes[3].push(a);
        }
    }
    let mut tmp = vec![0f32; w * h];
    let mut scratch = vec![0f32; w * h];
    for plane in planes.iter_mut() {
        box_blur(plane, &mut tmp, &mut scratch, w, h, r);
        box_blur(&tmp, plane, &mut scratch, w, h, r);
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let a = planes[3][i].clamp(0.0, 1.0);
            let (rr, gg, bb) = if a > 0.0 {
                (planes[0][i] / a, planes[1][i] / a, planes[2][i] / a)
            } else {
                (0.0, 0.0, 0.0)
            };
            let o = (((y0 + y as u32) * own.width + (x0 + x as u32)) * 4) as usize;
            own.data[o] = quantise(rr as f64 * 255.0);
            own.data[o + 1] = quantise(gg as f64 * 255.0);
            own.data[o + 2] = quantise(bb as f64 * 255.0);
            own.data[o + 3] = quantise(a as f64 * 255.0);
        }
    }
}

/// A blurred copy of the layer's coverage, added to the frame as light.
///
/// Only `rect` is considered: the layer's box grown by the radius. Everything outside it is
/// known to be unlit.
fn glow(
    canvas: &mut Canvas,
    own: &Canvas,
    rect: [u32; 4],
    radius: f64,
    strength: f64,
    color: Rgba,
) {
    if strength <= 0.0 {
        return;
    }
    let [x0, y0, x1, y1] = rect;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let r = (radius.max(1.0) / 3.0).round().max(1.0) as usize;
    let mut a: Vec<f32> = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            a.push(own.get(x0 + x as u32, y0 + y as u32).a as f32 / 255.0);
        }
    }
    let mut tmp = vec![0f32; a.len()];
    let mut scratch = vec![0f32; a.len()];
    // Three box passes are close enough to a gaussian that the falloff reads smooth.
    for _ in 0..3 {
        box_blur(&a, &mut tmp, &mut scratch, w, h, r);
        box_blur(&tmp, &mut a, &mut scratch, w, h, r);
    }
    for (i, cover) in a.iter().enumerate() {
        let cover = (cover * strength as f32).min(1.0) as f64;
        if cover <= 0.002 {
            continue;
        }
        let (px, py) = (x0 + (i % w) as u32, y0 + (i / w) as u32);
        canvas.composite(
            px,
            py,
            Rgba::opaque(color.r, color.g, color.b),
            cover,
            Blend::Screen,
        );
    }
}

/// Separable box blur with a sliding window, so the cost does not depend on the radius.
///
/// Each pixel costs two adds: one sample leaving the window and one entering it. Summing
/// the window per pixel instead turns a 40 px glow into hundreds of millions of adds, which
/// is how this first measured 614 ms a frame.
fn box_blur(src: &[f32], dst: &mut [f32], scratch: &mut [f32], w: usize, h: usize, r: usize) {
    if w == 0 || h == 0 {
        return;
    }
    // Zero padding, not clamped edges: outside the box the layer has no coverage, so a
    // clamped edge would invent light along the crop boundary.
    let norm = 1.0 / (2 * r + 1) as f32;
    for y in 0..h {
        let row = y * w;
        let mut sum: f32 = (0..=r.min(w - 1)).map(|x| src[row + x]).sum();
        scratch[row] = sum * norm;
        for x in 1..w {
            let leaving = x as i64 - 1 - r as i64;
            if leaving >= 0 {
                sum -= src[row + leaving as usize];
            }
            let entering = x + r;
            if entering < w {
                sum += src[row + entering];
            }
            scratch[row + x] = sum * norm;
        }
    }
    for x in 0..w {
        let mut sum: f32 = (0..=r.min(h - 1)).map(|y| scratch[y * w + x]).sum();
        dst[x] = sum * norm;
        for y in 1..h {
            let leaving = y as i64 - 1 - r as i64;
            if leaving >= 0 {
                sum -= scratch[leaving as usize * w + x];
            }
            let entering = y + r;
            if entering < h {
                sum += scratch[entering * w + x];
            }
            dst[y * w + x] = sum * norm;
        }
    }
}

/// Every layer id that some layer in `c` uses as a matte.
fn matte_sources(c: &kinema_doc::model::Comp) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for layer in &c.layers {
        if let Some(m) = &layer.mattes {
            for src in [m.alpha.as_ref(), m.luma.as_ref()].into_iter().flatten() {
                set.insert(src.clone());
            }
        }
    }
    set
}

/// Draw one layer's content into a canvas. `false` means the layer had nothing to draw.
#[allow(clippy::too_many_arguments)]
fn draw_layer(
    canvas: &mut Canvas,
    doc: &Doc,
    parent: &str,
    layer: &kinema_doc::Layer,
    ev: &kinema_doc::EvaluatedLayer,
    scale: f64,
    t: f64,
    mode: Blend,
    stack: &[String],
    depth: usize,
) -> Result<bool, RenderError> {
    let alpha = ev.opacity;
    let [bx, by, bw, bh] = ev.bbox;
    let (bx, by, bw, bh) = (bx * scale, by * scale, bw * scale, bh * scale);
    match layer.kind {
        LayerKind::Solid => {
            let (color, gradient) = match &layer.solid {
                Some(s) => (s.color, s.gradient.as_ref()),
                None => (Rgba::BLACK, None),
            };
            let (w, h) = (canvas.width as f64, canvas.height as f64);
            match gradient {
                Some(g) => canvas.fill_rect_gradient(0.0, 0.0, w, h, g, alpha, mode),
                None => canvas.fill_rect(0.0, 0.0, w, h, color, alpha, mode),
            }
        }
        LayerKind::Shape => {
            let Some(shape) = &layer.shape else {
                return Ok(false);
            };
            match (&shape.gradient, shape.kind) {
                (Some(g), kinema_doc::ShapeKind::Ellipse) => {
                    canvas.fill_ellipse_gradient(bx, by, bw, bh, g, alpha, mode)
                }
                (Some(g), _) => canvas.fill_rect_gradient(bx, by, bw, bh, g, alpha, mode),
                (None, kinema_doc::ShapeKind::Ellipse) => {
                    canvas.fill_ellipse(bx, by, bw, bh, shape.color, alpha, mode)
                }
                (None, _) => canvas.fill_rect(bx, by, bw, bh, shape.color, alpha, mode),
            }
        }
        LayerKind::Text => {
            let Some(text) = &layer.text else {
                return Ok(false);
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
                mode,
                anim: text.anim.as_ref(),
                time: t - layer.start,
            });
        }
        LayerKind::Comp => {
            // A nested comp is drawn on its own clock and placed by its bbox: the
            // layer's start is the nested comp's zero, and its position is where the
            // nested frame sits. Shutter stays with the comp being rendered.
            let Some(src) = &layer.source else {
                return Err(RenderError::Comp {
                    layer: layer.id.clone(),
                    source: String::new(),
                    why: format!("layer `{}` is a comp layer with no source", layer.id),
                    fix_op: None,
                });
            };
            if src == parent || stack.iter().any(|s| s == src) {
                return Err(RenderError::Comp {
                    layer: layer.id.clone(),
                    source: src.clone(),
                    why: format!("comp `{src}` contains itself through layer `{}`", layer.id),
                    fix_op: Some(
                        json!({ "op": "delete-layer", "comp": parent, "layer": layer.id, "reason": "break the cycle" }),
                    ),
                });
            }
            if !doc.comps.contains_key(src) {
                return Err(RenderError::Comp {
                    layer: layer.id.clone(),
                    source: src.clone(),
                    why: format!(
                        "layer `{}` points at `{src}`, which is not a comp",
                        layer.id
                    ),
                    fix_op: Some(
                        json!({ "op": "delete-layer", "comp": parent, "layer": layer.id, "reason": "drop a comp layer with nowhere to point" }),
                    ),
                });
            }
            let mut inner = stack.to_vec();
            inner.push(parent.to_string());
            let (nested, _, _) = draw_at(doc, src, t - layer.start, scale, &inner, depth + 1)?;
            canvas.composite_at(&nested, bx, by, alpha, mode);
        }
        other => {
            return Err(RenderError::Unsupported {
                kind: other.as_str().to_string(),
                layer: layer.id.clone(),
            })
        }
    }
    Ok(true)
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
            let (cv, layers, skipped) = draw_at(doc, comp, sub, scale, &[], 0)?;
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
        let (cv, layers, skipped) = draw_at(doc, comp, t, scale, &[], 0)?;
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
        matte_sources: matte_sources(c).into_iter().collect(),
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinema_doc::Doc;

    fn base_layer(id: &str, kind: LayerKind) -> kinema_doc::model::Layer {
        kinema_doc::model::Layer {
            id: id.into(),
            kind,
            start: 0.0,
            duration: 4.0,
            transform: kinema_doc::Transform::default(),
            opacity: kinema_doc::Prop::number(1.0),
            blend: Blend::Normal,
            solid: None,
            text: None,
            shape: None,
            mattes: None,
            modifiers: vec![],
            source: None,
        }
    }

    fn solid(id: &str, color: Rgba) -> kinema_doc::model::Layer {
        let mut l = base_layer(id, LayerKind::Solid);
        l.solid = Some(kinema_doc::Solid {
            color,
            gradient: None,
        });
        l
    }

    fn rect(id: &str, size: [f64; 2], color: Rgba) -> kinema_doc::model::Layer {
        let mut l = base_layer(id, LayerKind::Shape);
        l.shape = Some(kinema_doc::Shape {
            kind: kinema_doc::ShapeKind::Rect,
            size,
            color,
            radius: 0.0,
            gradient: None,
        });
        l
    }

    fn ellipse(id: &str, size: [f64; 2], color: Rgba) -> kinema_doc::model::Layer {
        let mut l = base_layer(id, LayerKind::Shape);
        l.shape = Some(kinema_doc::Shape {
            kind: kinema_doc::ShapeKind::Ellipse,
            size,
            color,
            radius: 0.0,
            gradient: None,
        });
        l
    }

    fn comp_of(layers: Vec<kinema_doc::model::Layer>) -> Doc {
        let mut doc = Doc::starter();
        let c = doc.comps.get_mut("main").unwrap();
        c.size = [320, 180];
        c.duration = 4.0;
        c.shutter = 0.0;
        c.layers = layers;
        doc
    }

    #[test]
    fn a_glow_puts_light_around_a_layer() {
        let mut bar = rect("bar", [40.0, 40.0], Rgba::opaque(255, 255, 255));
        bar.modifiers = vec![kinema_doc::Modifier {
            kind: "glow".into(),
            params: json!({"radius": 30.0, "strength": 0.8, "color": "#ffffff"})
                .as_object()
                .unwrap()
                .clone(),
        }];
        let doc = comp_of(vec![solid("bg", Rgba::opaque(0, 0, 0)), bar]);
        let out = render_frame(&doc, "main", 0, 1.0).unwrap();
        let px = |x: u32, y: u32| out.pixels[((y * out.width + x) * 4) as usize];
        assert!(px(185, 90) > 5, "the glow reaches past the bar");
        assert!(px(185, 90) < 250, "but it falls off");
        assert!(px(4, 4) < 5, "and it does not reach the corner");
    }

    #[test]
    fn a_blur_softens_the_edge_of_a_layer() {
        let mut bar = rect("bar", [40.0, 40.0], Rgba::opaque(255, 255, 255));
        bar.modifiers = vec![kinema_doc::Modifier {
            kind: "blur".into(),
            params: serde_json::json!({"radius": 18.0})
                .as_object()
                .unwrap()
                .clone(),
        }];
        let doc = comp_of(vec![solid("bg", Rgba::opaque(0, 0, 0)), bar]);
        let out = render_frame(&doc, "main", 0, 1.0).unwrap();
        let px = |x: u32, y: u32| out.pixels[((y * out.width + x) * 4) as usize];
        assert!(px(160, 90) > 200, "the middle stays bright");
        assert!(px(183, 90) > 5, "light spills past the bar's edge");
        assert!(px(4, 4) < 5, "and not to the corner");
    }

    #[test]
    fn an_unknown_modifier_is_an_error() {
        let mut bar = rect("bar", [10.0, 10.0], Rgba::opaque(255, 255, 255));
        bar.modifiers = vec![kinema_doc::Modifier {
            kind: "bloom".into(),
            params: serde_json::Map::new(),
        }];
        let doc = comp_of(vec![bar]);
        let err = render_frame(&doc, "main", 0, 1.0).unwrap_err();
        assert_eq!(err.to_json()["code"], "modifier");
    }

    #[test]
    fn a_comp_layer_draws_the_nested_comp() {
        let mut doc = comp_of(vec![]);
        doc.comps.insert(
            "inner".to_string(),
            kinema_doc::Comp {
                size: [100, 100],
                duration: 4.0,
                shutter: 0.0,
                samples: 8,
                layers: vec![solid("nested", Rgba::opaque(255, 0, 0))],
            },
        );
        let mut nested = base_layer("look", LayerKind::Comp);
        nested.source = Some("inner".into());
        doc.comps.get_mut("main").unwrap().layers = vec![nested];
        let out = render_frame(&doc, "main", 0, 1.0).unwrap();
        let inside = ((90 * out.width + 160) * 4) as usize;
        assert_eq!(
            &out.pixels[inside..inside + 3],
            &[255, 0, 0],
            "the nested comp is drawn where the layer sits"
        );
        // The nested frame is 100x100 in a 320x180 comp, so the corner stays empty.
        assert_eq!(out.pixels[3], 0, "nothing outside the nested frame");
    }

    #[test]
    fn a_comp_layer_that_contains_itself_is_refused() {
        let mut looped = base_layer("loop", LayerKind::Comp);
        looped.source = Some("main".into());
        let doc = comp_of(vec![looped]);
        let err = render_frame(&doc, "main", 0, 1.0).unwrap_err();
        assert_eq!(err.to_json()["code"], "comp", "{}", err.to_json());
    }

    #[test]
    fn an_alpha_matte_cuts_the_layer_and_hides_its_source() {
        // A small bar, so the frame shows what the matte did rather than covering it.
        let mut fill = rect("fill", [40.0, 20.0], Rgba::opaque(0, 0, 0));
        fill.mattes = Some(kinema_doc::Matte {
            alpha: Some("mask".into()),
            luma: None,
        });
        let doc = comp_of(vec![
            solid("bg", Rgba::opaque(255, 255, 255)),
            ellipse("mask", [100.0, 80.0], Rgba::opaque(0, 200, 0)),
            fill,
        ]);
        let out = render_frame(&doc, "main", 0, 1.0).unwrap();
        let px = |x: u32, y: u32| -> [u8; 4] {
            let o = ((y * out.width + x) * 4) as usize;
            [
                out.pixels[o],
                out.pixels[o + 1],
                out.pixels[o + 2],
                out.pixels[o + 3],
            ]
        };
        assert!(px(160, 90)[0] < 10, "inside the matte is the layer");
        // Inside the matte but outside the layer: the mask layer must not have drawn itself.
        assert!(
            px(160, 55)[0] > 200,
            "the matte source stays out of the frame, got {:?}",
            px(160, 55)
        );
        assert!(px(4, 4)[0] > 200, "outside the matte is the background");
    }

    #[test]
    fn a_luma_matte_keeps_half_the_coverage() {
        let mut fill = solid("fill", Rgba::opaque(0, 0, 0));
        fill.mattes = Some(kinema_doc::Matte {
            alpha: None,
            luma: Some("mask".into()),
        });
        let doc = comp_of(vec![
            solid("bg", Rgba::opaque(255, 255, 255)),
            solid("mask", Rgba::opaque(128, 128, 128)),
            fill,
        ]);
        let out = render_frame(&doc, "main", 0, 1.0).unwrap();
        let o = ((90 * out.width + 160) * 4) as usize;
        let value = out.pixels[o];
        assert!(
            (118..=138).contains(&value),
            "a mid-grey luma matte halves the coverage over white, got {value}"
        );
    }

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

    fn low_contrast_ramp(dither: bool) -> kinema_doc::Gradient {
        kinema_doc::Gradient {
            kind: kinema_doc::GradientKind::Linear,
            angle: 0.0,
            stops: vec![
                kinema_doc::Stop {
                    at: 0.0,
                    color: Rgba::opaque(0x10, 0x12, 0x18),
                },
                kinema_doc::Stop {
                    at: 1.0,
                    color: Rgba::opaque(0x16, 0x18, 0x20),
                },
            ],
            dither,
        }
    }

    #[test]
    fn dithering_breaks_the_bands_in_a_ramp() {
        let longest_run = |canvas: &Canvas| {
            let (mut run, mut best) = (1usize, 1usize);
            let mut prev = canvas.get(0, 0);
            for x in 1..canvas.width {
                let px = canvas.get(x, 0);
                if px == prev {
                    run += 1;
                } else {
                    run = 1;
                    prev = px;
                }
                best = best.max(run);
            }
            best
        };
        let mut plain = Canvas::new(800, 4);
        plain.fill_rect_gradient(
            0.0,
            0.0,
            800.0,
            4.0,
            &low_contrast_ramp(false),
            1.0,
            Blend::Normal,
        );
        let mut dithered = Canvas::new(800, 4);
        dithered.fill_rect_gradient(
            0.0,
            0.0,
            800.0,
            4.0,
            &low_contrast_ramp(true),
            1.0,
            Blend::Normal,
        );
        assert!(
            longest_run(&plain) > 32,
            "the undithered ramp should show long flat runs, got {}",
            longest_run(&plain)
        );
        assert!(
            longest_run(&dithered) < longest_run(&plain) / 2,
            "dithering should break the runs: {} vs {}",
            longest_run(&dithered),
            longest_run(&plain)
        );
    }

    #[test]
    fn a_radial_gradient_darkens_outwards() {
        let mut canvas = Canvas::new(200, 200);
        let g = kinema_doc::Gradient {
            kind: kinema_doc::GradientKind::Radial,
            angle: 0.0,
            stops: vec![
                kinema_doc::Stop {
                    at: 0.0,
                    color: Rgba::WHITE,
                },
                kinema_doc::Stop {
                    at: 1.0,
                    color: Rgba::BLACK,
                },
            ],
            dither: false,
        };
        canvas.fill_ellipse_gradient(0.0, 0.0, 200.0, 200.0, &g, 1.0, Blend::Normal);
        assert!(canvas.get(100, 100).r > 240, "centre is the first stop");
        assert!(canvas.get(2, 100).r < 80, "the rim is the last stop");
        assert!(canvas.get(0, 0).a == 0, "outside the ellipse stays clear");
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
