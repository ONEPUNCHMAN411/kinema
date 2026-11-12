# Kinema

Headless motion engine for AI agents. CLI `kin`, crates `kinema-*`.

There is no human UI and won't be one. Every surface is a tool call, a schema, or a
checkable result.

## Design

Code-first engines render a *projection of code*; After Effects renders a *document*.
Agents write code well but can't inspect it, and patch documents well. So Kinema keeps
one typed document, offers code frontends, and evaluates it in one deterministic pass.

- Calls are stateless: everything reconstructs from the document plus the op journal.
- Writes are CAS-versioned (`--expect-version`) so several agents can work on one project.
- Output is compact JSON, with `--fields` for projection.
- Errors are `{code, message, hint, fix_op, doc}` and `fix_op` is directly applicable.
- Every op carries a `reason`, so a critic agent can audit another agent's work.

## What it takes from where

| Source | Taken |
| --- | --- |
| RAEA (FSL, docs only) | browserless GPU frames, NV12 → hardware encode, dependency-tracked incremental bake, timeline as queryable data |
| HyperFrames (Apache-2.0) | declarative timing, seek-don't-play determinism, nested comps, typed ops + undo, the skill contract |
| Remotion | composition registry, input props, frame-pure render, chunked resumable render, parallel encode |
| After Effects | precomps, bezier easing, expressions, mattes, adjustment layers, time remap, motion blur, shape ops, text animators, Essential Properties |
| Blender | ID datablocks with linking, depsgraph, node graphs as data, modifier stack, AOV/EXR/OCIO |

The craft layer — type, colour, motion, audio and the detector suite — is what makes
output look professional rather than generated. It is specified in `docs/PLAN.md`.

## Status: P0

Built:

- `kinema-time` — rational frame rate, 90 kHz tick timeline, timecode.
- `kinema-expr` — the expression language: no loops, no assignment, no I/O, bounded
  source, depth and steps, with `t`, `frame`, easing, springs, deterministic noise and
  `link()` into any other property.
- `kinema-doc` — the document, properties (`value | kf | expr | link | graph`), typed ops
  with inverses, and an append-only journal.
- `kinema-deps` — canonical JSON and content hashes for documents, layers and frames.
- `kinema-raster` — CPU RGBA canvas, blend modes, bitmap text, PNG out.
- `kinema-checks` — the first craft detectors with `fix_op` remedies.
- `kinema-cli` — `kin` itself, including asset records with looked-up licences and
  `undo` driven by the journal's stored inverses.

The document supports `solid`, `text` and `shape` layers; properties support constants,
keyframes, links and expressions. `graph` returns a typed `not-implemented` with a hint
that names the phase. Text is drawn with a built-in 5 x 7 bitmap face — `Inter` at 400 and 700
are the only declared faces, and `TYPE-FAKE` says so when a document asks for others.

Not built yet: GPU raster, media decode, encode, audio, assets, capture, templates, MCP.
Each returns a typed `not-implemented` error with a hint rather than pretending.

Detectors live in this build: `TYPE-FAKE`, `READ`, `CHROMA`, `PALETTE`, `EASE-FLAT`,
`HOLD-NONE` and `STAGGER`. All but `HOLD-NONE` ship a `fix_op`, and `kin check --fix`
applies them to a fixpoint: remedies are measured against one snapshot, so CHROMA's
remedy and READ's can disagree, and only re-measuring after each step settles it. The
remaining detectors are specified with their thresholds in `docs/PLAN.md` and land with
the features they measure.

![The starter project rendered at frame 30](docs/first-still.png)

`kin new demo` then `kin render --frame 30`: the CPU tier's first still. The title and the
rule both carry bezier keyframes, the three entrances are staggered, and `kin check` is
clean.

## Usage

```sh
kin new demo                     # scaffold a project
kin capabilities                 # what this build can do
kin schema                       # JSON Schema for the document
kin apply --project demo --ops ops.json --expect-version 0
kin probe --project demo --at 1.0    # evaluated layer state at a time
kin eval 'sin(t * 2) * 40' --at 0.5  # one expression, on its own
kin render --project demo --frame 30 -o demo/f30.png
kin check --project demo         # craft defects with fix_op remedies
kin check --project demo --fix   # apply them until nothing fixable is left
kin credits --project demo       # licence summary and release gate
kin log --project demo           # the op journal
kin credits --project demo       # licence roll-up and release gate
kin asset add still.png --project demo --spdx cc-by-4.0 --source-url <url> --author <name>
kin undo --project demo          # replay the journal's inverses
kin render --project demo --frames 0-59 -o demo/frames   # sequence + hash manifest
```

Every verb prints JSON. Errors carry `code`, `message`, `hint`, `fix_op` and `doc`:

```json
{"ok":false,"error":{"code":"version-conflict","message":"document is at version 4, you expected 0","hint":"rebase: reload with `kin probe`, re-read the value, and retry with the current version","fix_op":null,"doc":"docs/PLAN.md#contract"}}
```

## Build

```sh
cargo build --release
cargo test
```

## Licence

MIT OR Apache-2.0. No GPL code is linked; x264/x265 stay behind an opt-in feature, and
Blender is only ever invoked as a subprocess.
