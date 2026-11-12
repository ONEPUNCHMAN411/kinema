# Kinema — session handoff

Written 2026-11-12. This is the brief for whoever (or whatever) picks Kinema up next.

## What Kinema is

A headless motion-graphics engine for AI agents. No GUI, no timeline, no mouse: a typed
document plus typed ops, a deterministic CPU renderer, and craft detectors that ship the
`fix_op` that repairs what they find.

- Repository: `github.com/ONEPUNCHMAN411/kinema`
- Licence: MIT OR Apache-2.0
- Language: Rust, workspace of ten crates, CLI binary named `kin`
- Windows dev host in this session; CI runs `ubuntu-latest` and `windows-latest`

## The plan

- `docs/PLAN-ORIGINAL.md` — the original plan, verbatim and never edited. Treat it as the
  brief; do not "clean it up".
- `docs/PLAN.md` — the working plan: phases, detector table, contracts. This is the one
  that gets updated as things ship.
- `README.md` — the public status: what actually works today, with the limits stated.

Detector table in `docs/PLAN.md` is the source of truth for what each detector measures and
which phase it belongs to.

## Standing preferences from the user

These were said explicitly and shape every session:

1. **Headless only, and only agents use it.** Every affordance exists for a program
   reading JSON, not a person looking at a panel.
2. **Commit in larger steps rather than small steps.** One commit per coherent feature
   with a real message, not a commit per file edit.
3. **"Continue, don't stop."** Keep shipping until told otherwise.
4. **Chase a commit count.** Near the end of this session: "don't stop working til you
   have 30 commits done". The repository was already at 34 commits when that was said, so
   the literal target was met; the spirit was "keep the commits coming".
5. **Commit footer, verbatim:**

   ```
   🤖 Generated with Codebuff
   Co-Authored-By: Codebuff <noreply@codebuff.com>
   ```

   Word commit messages so they contain no backticks: the message goes through a shell
   heredoc and backticks get expanded.

## Contracts that must not drift

- **Errors** are `{code, message, hint, fix_op, doc}`. A detector either ships a `fix_op`
  that a machine can apply, or is advisory with `fix_op: null` because the remedy is a
  directing call. No third option.
- **Every op carries a `reason`** and has an inverse; `kin apply` returns them.
- **Calls are stateless.** All state lives in `doc.json` plus the journal.
- **CAS:** `--expect-version` on writes.
- **Determinism:** 90 kHz ticks, integer frames, no wall clock, no ambient randomness.
- **Optimisations must be pixel-identical** unless the old behaviour was a bug; when a
  change cannot be, say so with numbers instead of claiming identity.

## Crate layout and layering

```
time → expr → text → type → palette → doc → deps → raster → checks → cli
```

- `text` does not depend on `doc`; `type` depends on `text`; `palette` depends on `doc`.
- `doc` must not depend on `palette` or `type`.
- `raster` is the `strict` CPU tier and the reference the future GPU tier is checked
  against, so it stays small and obviously correct.
- `kinema-doc/src/lib.rs` has `#![recursion_limit = "256"]` for the deep `json!` schema.

## State at handoff

- `HEAD` = `de5b5b5` (37 commits total), working tree has **one uncommitted change**
  (see "In flight" below).
- **165 tests pass**, `cargo clippy --all-targets -- -D warnings` is clean, CI green on
  every push made this session (latest run 1m17s).
- Demo projects live under `target/` and are gitignored: `target/grad`, `target/band`,
  `target/matte`, `target/pre`, `target/glow`, `target/mod`, `target/shadow`, plus the
  older `target/perf`, `target/fonts`, `target/faces`.

### In flight (uncommitted)

`crates/kinema-cli/src/main.rs` — `kin probe` now reports `source`, `modifiers`, `matte`
and `draws` per layer, so a matte source that will not draw itself is visible in the probe
instead of only in the render report. It compiles and the full suite passes (165); the
probe output itself was not eyeballed before the session stopped, so verify that first and
then commit it.

## What shipped this session, in order

| Commit | What | Why it mattered |
| --- | --- | --- |
| `029e83c` | memcpy full-canvas solids, glyph cache, lazy face parsing | 360 → 81 ms a frame at 8 subframes, 4.4×, golden hash unchanged |
| `bd3cdea` | PT Serif (400/700/italic), Abril Fatface, Bebas Neue; `editorial-serif`, `poster`, `condensed` | 10 faces / 5 families; verified by rendering |
| `ac816ce` | gradients + dither + `BANDING` + `set-gradient` | linear-light stops; dither applied before quantisation; 2294 → 292 ms |
| `db577e7` | cache the tabulated ramp | 292 → 117 ms; subframes reuse it, byte-identical |
| `c9e8d5b` | mattes (alpha/luma), `set-mattes`, `matte_sources` | cut a layer to another; source does not draw itself |
| `c162b6b` | nested comps | own clock, bbox placement, cycle → `fix_op`, nested stays sharp |
| `713dd84` | `create-comp` / `delete-comp` | comps authorable through ops; delete's inverse restores layers |
| `1190c38` | `glow` + refusal for unknown modifiers | 652 → 119 ms, sliding-window blur, zero-padded edges |
| `2a24818` | `blur` + `set-modifier` | premultiplied before blur so soft edges keep their colour |
| `7d0ecd5` | `drop-shadow` | shares one silhouette path with glow; offset sampling |
| `53ba532` | `MATTE-COVER`, `EMPTY-COMP` detectors | the two ways a composed frame silently comes out empty |
| `f2905a3` | exposure / contrast / saturate + adjustment layers | grades in linear light; adjustment bbox bug fixed |
| `de5b5b5` | `MODIFIER` detector | catches an unknown modifier before a render, ships the fix |

## Hard-won gotchas

- **A matte hides its source layer.** That is deliberate (AE-like), and `kin render`
  reports it in `matte_sources`. The twin trap: a matte whose source ink does not reach the
  layer is legitimate and silent — the bbox can even overlap by a pixel and still be
  empty. Read both boxes with `kin probe` before wondering where the picture went. This is
  what `MATTE-COVER` now catches.
- **Dither must be applied before quantisation.** Adding a bias to an 8-bit value can only
  return that value; the test that failed here is why `Gradient::sample_linear` exists.
- **Interpolate in linear light**, never on encoded values: black-to-white otherwise goes
  grey in the middle.
- **Crop the work, not the output.** The glow was 652 ms because it blurred and blended the
  whole canvas; restricting to the layer's box plus radius made it 119 ms. The first crop
  clamped at the edge and invented light there — zero padding is the correct boundary.
- **Float summation order matters a little.** The cropped glow differs from the whole-canvas
  one on 2981 of 627k pixels by at most 2/255. Rounding, not structure; stated in the commit
  rather than hidden.
- **`cargo fmt` reindents, so re-read a region before editing it.** Several `str_replace`
  misses this session came from stale indentation.
- **Never `write_file` over an existing source file** unless the whole content is in hand;
  it truncated `kinema-doc/src/lib.rs` once and had to be recovered with `git checkout --`.

## Next steps, roughly in value order

1. **Audio.** `kinema-audio`: parse a WAV (PCM 16/24/32, float 32), report duration,
   sample rate, channels, peak dBFS and RMS. Then `LOUD` can measure something honest.
   Be explicit that RMS is not EBU R128 LUFS until K-weighting is actually implemented.
2. **`CUT-TIMING` / `JUMPCUT`.** Both need visibility analysis the model now supports
   (comp layers exist, so a nested comp's cuts are computable).
3. **`image` layers.** Needs a decoder. Decide whether a vendored PNG decoder is acceptable
   before promising the layer kind.
4. **`kin render --frames` reuse across processes.** The ramp cache and glyph cache are
   per-process; a sequence through the CLI pays for a cold process per frame (184 ms/frame
   measured on a gradient comp).
5. **`text.anim` "pop".** A scale-in per unit needs glyph rasterisation at a per-frame size,
   which misses the glyph cache each frame; measure before shipping.
6. **Docs drift.** `docs/PLAN.md` detector table and the phase rows still describe the
   original schedule; bring it in line with what has actually shipped.

## How to verify a change

```bash
export PATH="$USERPROFILE/.cargo/bin:$PATH"   # Windows dev host
cd 01-Projects/kinema
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release

# render something and look at it
./target/release/kin.exe new target/demo --force
./target/release/kin.exe apply --project target/demo --ops ops.json
./target/release/kin.exe check --project target/demo
./target/release/kin.exe render --project target/demo --frame 45 --scale 0.55 -o target/demo/f.png
```

A visual change you did not look at is unverified. Say so, do not imply you checked it.

## Where the story is

The interesting thread of this session was not features but **silent lies**: things the
document could say that the renderer quietly ignored — mattes, comp layers, modifiers, an
adjustment layer with a zero-sized box. Each one was fixed by making it real or making it
an error with a `fix_op`, and the two detectors shipped at the end (`MATTE-COVER`,
`EMPTY-COMP`, `MODIFIER`) exist because those silences cost time to diagnose. Keep that bar:
if a document can ask for something, either draw it or refuse it loudly.
