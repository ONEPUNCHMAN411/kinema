# Kinema — the plan

Kinema is a headless motion engine for AI agents. CLI `kin`, crates `kinema-*`, no
reused code. There is no human UI: every surface is a tool call, a schema, or a
checkable result.

## Design

Code-first engines render a *projection of code*; After Effects renders a *document*.
Agents write code well but can't inspect it, and patch documents well. So Kinema keeps
one typed document, offers code frontends, and evaluates it in one deterministic pass.

- **One typed document.** Everything — comps, layers, properties, assets, licences,
  look — is a value in `doc.json`. Code frontends compile to it; the evaluator only
  ever sees it.
- **Blender's depsgraph.** Layers are datablocks; a layer's output is a pure function of
  its inputs at a tick. Change one input and only its dependants are re-evaluated.
- **Hash-based incrementality.** A frame is named by a hash of everything it depends on,
  so "re-render only what changed" is derived rather than declared.
- **Stateless calls.** Everything reconstructs from the document plus the op journal.
- **CAS writes.** `--expect-version` makes every write compare-and-swap, so parallel
  agents collide loudly instead of silently.
- **Compact JSON**, `--fields` for projection.
- **Errors are `{code, message, hint, fix_op, doc}`** and `fix_op` is directly
  applicable. Every op carries a `reason`, so a critic agent can audit another's work.
- **Honesty.** An unbuilt verb returns typed `not-implemented` with a hint. It never
  fakes success.

## What it takes from where

| Source | Taken |
| --- | --- |
| RAEA (FSL, docs only) | browserless GPU frames, NV12 → hardware encode, dependency-tracked incremental bake, timeline as queryable data |
| HyperFrames (Apache-2.0) | declarative timing, seek-don't-play determinism, nested comps, typed ops + undo, the 9-skill contract |
| Remotion | composition registry, `inputProps`, frame-pure render, chunked resumable render, parallel encode |
| After Effects | precomps, bezier editor, expressions, mattes, adjustment layers, time remap, motion blur, shape ops, text animators, Essential Properties |
| Blender | ID datablocks with linking, depsgraph, node graphs as data, modifier stack, AOV/EXR/OCIO |

## Type and colour

Agents default to *italics on a purple gradient* because they have neither faces nor
tokens. Kinema supplies both.

- `kin fonts` ships ~40 OFL families with classification, x-height, contrast, variable
  axes, subsets, licence and pairings. `font add google:Name|<file>` captures licence and
  attribution.
- **Fake italic, bold and small caps are rejected** (`TYPE-FAKE`). Synthesised oblique is
  the tell.
- `typeface recommend --for "editorial explainer" --roles display,body,mono` returns real
  pairings with reasons. `type set --scale 1.25` applies a modular scale with per-size
  tracking and leading.
- `palette from <image|brand|video>` extracts in OKLab, assigns roles (bg, surface, text,
  muted, accent), gamut-maps to Rec.709 so video never clips, and proves every
  text-on-surface pair with contrast ratios. One accent, one neutral ramp, no rainbow
  gradients.
- A **look** binds palette + type + curves + sound look, so a project reads as one design.

## Motion taste

**Skills** — `craft-contract`, `shot-composition`, `kinetic-type`, `captions`,
`lower-thirds`, `data-story`, `beat-sync`, `3d-stage`, `critique`, `polish-pass`. Each
follows HyperFrames' contract: trigger → procedure → exact tables → anti-patterns →
example → `assert` gates. After LottieFiles' checklist and iart-ai's packs; `skills add`
installs MIT packs behind `skills vet`.

**Corpus** (DTCG tokens) — Carbon 70–700 ms and curves, M3 springs, snappy/gentle/bounce,
1–3 frame staggers, enter 500 / exit 200.

**Judge** — anchored rubric, evidence per criterion, skeptic pass, blind pairwise,
calibration set, subtraction pass.

## Templates and reuse

Reusables are datablocks with ID, version and licence: comp, component, **transition**,
effect, animation preset, look, sound look, capture recipe, shot recipe.

Manifest fields: typed `params`, `slots`, `expose`, `requires`, `presets`, `bounds`,
`asserts`, `examples`, `licence`.

`save --from comp:main/layer:intro` extracts a reusable and promotes literals to slots;
`apply --at 0:12` instantiates it with overrides. Instances link by ID, so a library fix
re-renders only affected frames.

**Transitions** take two inputs, declare `handles`, verify them (freeze/stretch
fallback) and land on the beat. `library promote` lifts approved work.

## Assets

`assets search` ranks by fitness, not popularity: slot aspect, duration, ≥2× pixels,
alpha, isolation, palette, style, loopability, BPM — returning preview, `describe`, size
and licence.

Sources: Poly Haven (CC0 API, named UA + credit), ambientCG, Poly Pizza, Kenney,
Quaternius, Openverse, Wikimedia, Freesound, Pexels, Sketchfab CC, Google Fonts, Iconify.

Licence is a datablock field:

```json
{"spdx": "CC0-1.0", "source_url": "…", "author": "…", "retrieved_at": "2026-11-12",
 "attribution_required": false, "commercial_ok": true, "derivatives_ok": true,
 "share_alike": false, "model_release": false}
```

`license <id>` gives plain language plus the credit line; `--release` fails on `unknown`
or a missing credit; `credits` emits `credits.json`, an end-slate and a description.
Never guessed. `verify` checks dimensions, alpha, frames, audio, burned-in text,
watermarks, NSFW, duplicates, and that the licence URL resolves.

## Capture

`capture` → deterministic frames: `url` (headless, fixed viewport/DPR/fonts, motion
frozen, overlays stripped, 2×), `selector`, `--steps` walkthroughs as stepped frames,
`terminal --cmd`, `code --diff`, `blender --turntable`.

`--check` catches upscaled blur, illegible text, clipping, spinners or banners mid-state.
Rights travel with the capture; public sites are `redistribute=review` until
acknowledged; PII is auto-redacted.

## Audio

Agents cannot hear, so the engine hears for them. `audio hear <f>` returns one bounded
bundle: per-second LUFS/true peak, LRA, band energies, noise floor, hum, clipping, width,
mono-compat, VAD, diarisation, word timings, onsets, tempo/beat grid, key, sections,
reverb.

**Whoosh 1 vs whoosh 2.** Every sound gets a fingerprint (attack, centroid, slope,
low/high split, tonal vs noise, glide, RT60, width, loudness, loopability) and a **role
spec** decides fitness:

| Slot | Requires |
| --- | --- |
| riser into a cut | upward glide, tail ≤ lead-in |
| impact | < 10 ms attack, tail inside the beat |
| transition | noise-dominant mids |
| bed | loopable, no 1–4 kHz under dialogue |

`compare a b --slot cut3` loudness-matches, aligns onsets, prints differing axes in
numbers and words, names the winner and why. `--sheet` renders labelled spectrograms so a
multimodal agent can *see* what it cannot hear.

Traps caught: louder reads as better, near-duplicate SFX, repetition fatigue, key clash.

`mix --target youtube|podcast|broadcast` measures dialogue, beds −6 LU under, 80–150 /
300–500 ms duck, HPF 100 Hz, limits −1 dBTP, and reports every change. `synth` keeps
risers and ticks rights-clean; a `sound look` holds one library's character.

## Detectors

The document model kills per-shot regeneration tells; the rest is amateurism, and it is
measurable. Each gate is an `assert` with a `fix_op`.

| code | detector | status |
| --- | --- | --- |
| STAGGER | >70% of layers enter on one frame | **P0** |
| EASE-FLAT | linear/symmetric easing on moves >200 ms or >2% of frame | **P0** |
| HOLD-NONE | no still frame in 3 s; constant motion is the loudest tell | **P0** (advisory) |
| SHUTTER | >1.5 px/frame with <8 subframe samples; 180° = 1/(2·fps) | **P0** |
| CUT-TIMING | dialogue cut >6 frames after last speech (Ford: 35% within one) | P3 |
| JUMPCUT | same subject across <30° camera or <30% scale change | P3 |
| TYPE-FAKE | synthesised italic/bold/small caps, or a system fallback face | **P0** |
| PALETTE | out-of-Rec.709 colours, multiple accents, rainbow gradients | **P0** (accents) |
| CHROMA | saturated text <32 px on saturated ground; 4:2:0 destroys it | **P0** |
| BANDING | flat gradient >2% of frame, <2 codes/100 px, no dither | P3 |
| READ | contrast <4.5:1, outside title-safe, over busy unprotected area | **P0** (contrast) |
| LOUD | dialogue off −14/−16 LUFS, bed >−6 LU under, ducking or TP wrong | P5 |

## Build

Crates: `time doc deps graph anim expr raster(wgpu) text media audio encode`
(NVENC/QSV/AMF/VideoToolbox/openh264/rav1e + muxide) `cache render comp(EXR/OCIO) html
blender assets capture templates taste skills checks mcp cli`.

Tiers:

- **strict** — CPU, bit-identical.
- **gpu** — cross-machine SSIM ≥ 0.999.
- **fast** — throughput first.

CI runs 40-comp goldens, play == render, A/V clapper.

Public GitHub repo, MIT OR Apache-2.0, CI on push, tagged canaries. Conventional commits,
no secrets.

## Phases

| Phase | Weeks | Deliverable |
| --- | --- | --- |
| P0 | 0–2 | toolchain, repo, doc/deps, capabilities, CPU still |
| P1 | 2–7 | GPU compositor, decode, NVENC, first MP4 |
| P2 | 7–13 | keyframes, text, shapes, masks, audio graph, cache |
| P3 | 13–19 | node graphs, compositor + EXR/OCIO, check/assert |
| P4 | 19–26 | mattes, adjustment layers, expressions, text animators, fonts + palette, taste corpus, judge, templates |
| P5 | 26–32 | assets + licences, capture, audio fingerprints/compare, transitions, HTML/TSX, MCP |
| P6 | 32–38 | Blender bridge, pool, OTIO/Lottie, v1.0 |

## Environment

Windows, RTX 4060 (8 GB → tiles), NVENC primary. Rust 1.99.1 MSVC with the VS 2022 Build
Tools linker; no admin needed.

## The vertical slice

```sh
kin new demo
kin apply --project demo --ops ops.json
kin render --project demo --frame 0 -o demo/frame0.png
kin probe --project demo --at 1.0
kin check --project demo
```

That loop — write, apply, render, inspect, check — is the whole product. Everything after
P0 widens it; nothing replaces it.
