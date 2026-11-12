# Kinema — the approved plan (verbatim)

This is the plan as approved, unedited. It is the reference everything else is checked
against: if the working documentation in [`PLAN.md`](PLAN.md) or the README disagrees with
this file, this file wins and the other one is wrong.

Status of each item lives in `PLAN.md` and the README, never here.

---

# Kinema — headless motion engine for AI agents (Rust)

CLI `kin`, crates `kinema-*`, new project, no reused code.

## Design

No human UI: every surface is a tool call, schema or checkable result. Code-first engines
render a *projection of code*; AE renders a *document*. Agents write code well but can't
inspect it, and patch documents well. So: one typed document, code frontends, one
deterministic evaluator, Blender's depsgraph, plus a craft layer that supplies taste.
Calls are stateless (doc+journal), CAS-versioned for parallel agents, compact JSON,
errors as `{code,hint,fix_op}`, every op carrying `reason`.

## Take

- RAEA (FSL, docs only): browserless GPU frames, NV12→hardware encode,
  dependency-tracked incremental bake, timeline as queryable data.
- HyperFrames (Apache-2.0): declarative timing, seek-don't-play determinism, nested comps,
  typed ops+undo, the 9-skill contract.
- Remotion: composition registry, inputProps, frame-pure render, chunked resumable render,
  parallel encode.
- AE: precomps, bezier editor, expressions, mattes, adjustment layers, time remap, motion
  blur, shape ops, text animators, Essential Properties.
- Blender: ID datablocks with linking, depsgraph, node graphs as data, modifier stack,
  AOV/EXR/OCIO.

## Type & colour

Agents default to *italics on a purple gradient* because they have neither faces nor
tokens. `kin fonts` ships ~40 OFL families with classification, x-height, contrast,
variable axes, subsets, licence and pairings; `font add google:Name|<file>` captures
licence and attribution. **Fake italic, bold and small caps are rejected** (`TYPE-FAKE`) —
synthesised oblique is the tell. `typeface recommend --for "editorial explainer" --roles
display,body,mono` returns real pairings with reasons; `type set --scale 1.25` applies a
modular scale with per-size tracking and leading. `palette from <image|brand|video>`
extracts in OKLab, assigns roles (bg, surface, text, muted, accent), gamut-maps to Rec.709
so video never clips, and proves every text-on-surface pair with contrast ratios. One
accent, one neutral ramp, no rainbow gradients. A `look` binds palette + type + curves +
sound look, so a project reads as one design.

## Motion taste

Skills (`craft-contract`, `shot-composition`, `kinetic-type`, `captions`, `lower-thirds`,
`data-story`, `beat-sync`, `3d-stage`, `critique`, `polish-pass`): trigger → procedure →
exact tables → anti-patterns → example → `assert` gates. After HyperFrames' contract,
LottieFiles' checklist, iart-ai's packs; `skills add` installs MIT packs behind
`skills vet`. Corpus (DTCG): Carbon 70–700 ms and curves, M3 springs, snappy/gentle/bounce,
1–3 frame staggers, enter 500/exit 200. `judge`: anchored rubric, evidence per criterion,
skeptic pass, blind pairwise, calibration set, subtraction pass.

## Templates & reuse

Reusables are datablocks with ID, version, licence: comp, component, **transition**,
effect, animation preset, look, sound look, capture recipe, shot recipe. Manifest: typed
`params`, `slots`, `expose`, `requires`, `presets`, `bounds`, `asserts`, `examples`,
`licence`. `save --from comp:main/layer:intro` extracts and promotes literals to slots;
`apply --at 0:12` instantiates with overrides; instances link by ID, so a library fix
re-renders only affected frames. **Transitions** take two inputs, declare `handles`, verify
them (freeze/stretch fallback) and land on the beat. `library promote` lifts approved work.

## Assets

`assets search` ranks by fitness not popularity: slot aspect, duration, ≥2× pixels, alpha,
isolation, palette, style, loopability, BPM — returning preview, `describe`, size and
licence. Sources: Poly Haven (CC0 API, named UA + credit), ambientCG, Poly Pizza, Kenney,
Quaternius, Openverse, Wikimedia, Freesound, Pexels, Sketchfab CC, Google Fonts, Iconify.
Licence is a datablock field `{spdx, source_url, author, retrieved_at,
attribution_required, commercial_ok, derivatives_ok, share_alike, model_release}`;
`license <id>` gives plain language plus the credit line; `--release` fails on `unknown` or
a missing credit; `credits` emits credits.json, end-slate and description. Never guessed.
`verify` checks dimensions, alpha, frames, audio, burned-in text, watermarks, NSFW,
duplicates, and that the licence URL resolves.

## Capture

`capture` → deterministic frames: `url` (headless, fixed viewport/DPR/fonts, motion frozen,
overlays stripped, 2×), `selector`, `--steps` walkthroughs as stepped frames, `terminal
--cmd`, `code --diff`, `blender --turntable`. `--check` catches upscaled blur, illegible
text, clipping, spinners or banners mid-state. Rights travel with the capture; public sites
are `redistribute=review` until acknowledged; PII auto-redact.

## Audio

`audio hear <f>` → one bounded bundle: per-second LUFS/true peak, LRA, band energies, noise
floor, hum, clipping, width, mono-compat, VAD, diarisation, word timings, onsets, tempo/beat
grid, key, sections, reverb. **Whoosh 1 vs whoosh 2**: each sound gets a fingerprint
(attack, centroid, slope, low/high split, tonal vs noise, glide, RT60, width, loudness,
loopability) and a **role spec** decides fitness: riser into a cut needs upward glide and
tail ≤ lead-in; impact needs <10 ms attack and a tail inside the beat; transition needs
noise-dominant mids; bed needs loopable with no 1–4 kHz under dialogue. `compare a b --slot
cut3` loudness-matches, aligns onsets, prints differing axes in numbers and words, names the
winner and why; `--sheet` renders labelled spectrograms so a multimodal agent can *see* what
it cannot hear. Traps caught: louder reads as better, near-duplicate SFX, repetition
fatigue, key clash. `mix --target youtube|podcast|broadcast` measures dialogue, beds −6 LU
under, 80–150/300–500 ms duck, HPF 100 Hz, limits −1 dBTP, reports every change. `synth`
keeps risers and ticks rights-clean; a `sound look` holds one library's character.

## Detectors

The document model kills per-shot regeneration tells; the rest is amateurism, and it is
measurable — `assert` gates, each with a `fix_op`.

| code | detector |
|---|---|
| STAGGER | >70% of layers enter on one frame |
| EASE-FLAT | linear/symmetric easing on moves >200 ms or >2% of frame |
| HOLD-NONE | no still frame in 3 s; constant motion is the loudest tell |
| SHUTTER | >1.5 px/frame with <8 subframe samples; 180° = 1/(2·fps) |
| CUT-TIMING | dialogue cut >6 frames after last speech (Ford: 35% within one) |
| JUMPCUT | same subject across <30° camera or <30% scale change |
| TYPE-FAKE | synthesised italic/bold/small caps, or a system fallback face |
| PALETTE | out-of-Rec.709 colours, multiple accents, rainbow gradients |
| CHROMA | saturated text <32 px on saturated ground; 4:2:0 destroys it |
| BANDING | flat gradient >2% of frame, <2 codes/100 px, no dither |
| READ | contrast <4.5:1, outside title-safe, over busy unprotected area |
| LOUD | dialogue off −14/−16 LUFS, bed >−6 LU under, ducking or TP wrong |

## Build & repo

Crates: `time doc deps graph anim expr raster(wgpu) text media audio encode(NVENC/QSV/AMF/
VideoToolbox/openh264/rav1e+muxide) cache render comp(EXR/OCIO) html blender assets capture
templates taste skills checks mcp cli`. Tiers: strict (CPU, bit-identical), gpu
(cross-machine SSIM ≥0.999), fast; CI runs 40-comp goldens, play==render, A/V clapper.
Public GitHub repo under your account (`gh auth status` resolves the login at build; you
confirm the name), MIT OR Apache-2.0, README carrying this plan, CI on push, tagged
canaries. Commit small and often — per crate, per passing golden, per detector, per
template kind — conventional messages, no secrets.

## Plan

P0 W0–2 toolchain, repo, doc/deps, capabilities, CPU still. P1 W2–7 GPU compositor, decode,
NVENC, first MP4. P2 W7–13 keyframes, text, shapes, masks, audio graph, cache. P3 W13–19
node graphs, compositor+EXR/OCIO, check/assert. P4 W19–26 mattes, adjustment layers,
expressions, text animators, fonts+palette, taste corpus, judge, templates. P5 W26–32
assets+licences, capture, audio fingerprints/compare, transitions, HTML/TSX, MCP. P6 W32–38
Blender bridge, pool, OTIO/Lottie, v1.0.

Windows, RTX 4060 (8 GB → tiles), no rustc/cargo/ffmpeg/MSVC linker → rustup + VS Build
Tools first; NVENC primary. First commits: toolchain, six crates, doc+ops+schema, `new →
apply → render --frame 0 → probe`, then licence, template and audio records.
