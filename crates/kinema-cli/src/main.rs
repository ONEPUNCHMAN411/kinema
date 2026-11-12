//! `kin` — every surface is a tool call, a schema or a checkable result.
//!
//! Output is JSON on stdout for every verb. Failures print the same
//! `{code, message, hint, fix_op, doc}` envelope, so an agent never has to parse prose.

use kinema_checks as checks;
use kinema_deps as deps;
use kinema_doc::{schema, Doc, Op};
use kinema_raster as raster;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

type R = Result<Value, Value>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let pretty = has(&argv, "--pretty");
    match dispatch(&argv) {
        Ok(v) => {
            print_json(&project_fields(v, &argv), pretty);
        }
        Err(e) => {
            print_json(&e, pretty);
            std::process::exit(1);
        }
    }
}

/// `--fields a,b` keeps only the named top-level keys, for token economy. Errors are
/// never projected: an agent needs `code`, `hint` and `fix_op` whatever it asked for.
fn project_fields(v: Value, argv: &[String]) -> Value {
    let Some(spec) = get(argv, "--fields") else {
        return v;
    };
    let Value::Object(map) = v else { return v };
    let mut out = serde_json::Map::new();
    for key in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(value) = map.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(out)
}

fn print_json(v: &Value, pretty: bool) {
    let s = if pretty {
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into())
    } else {
        serde_json::to_string(v).unwrap_or_else(|_| "{}".into())
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{s}");
}

fn dispatch(argv: &[String]) -> R {
    let verb = argv.first().map(String::as_str).unwrap_or("help");
    match verb {
        "capabilities" => Ok(capabilities()),
        "fonts" | "typefaces" => Ok(cmd_fonts()),
        "type" | "scale" => Ok(cmd_type(argv)),
        "typeface" => Ok(cmd_typeface(argv)),
        "palette" => cmd_palette(argv),
        "schema" => Ok(schema::doc_schema()),
        "help" | "--help" | "-h" => Ok(help()),
        "version" | "--version" | "-V" => {
            Ok(json!({ "ok": true, "kin": VERSION, "schema": kinema_doc::model::SCHEMA_TAG }))
        }
        "new" => cmd_new(argv),
        "probe" => cmd_probe(argv),
        "eval" => cmd_eval(argv),
        "apply" => cmd_apply(argv),
        "render" => cmd_render(argv),
        "check" => cmd_check(argv),
        "credits" => cmd_credits(argv),
        "log" => cmd_log(argv),
        "undo" => cmd_undo(argv),
        "verify" => cmd_verify(argv),
        "diff" => cmd_diff(argv),
        "asset" | "assets" => cmd_asset(argv),
        other => Err(not_implemented(other)),
    }
}

fn not_implemented(verb: &str) -> Value {
    let (hint, phase) = match verb {
        "assets" | "licence" | "license" => (
            "asset search, licence datablocks and credits land in P5",
            "P5",
        ),
        "audio" => (
            "`kin audio hear/compare/mix` lands in P5; no audio crate is in this build",
            "P5",
        ),
        "capture" => ("deterministic capture lands in P5", "P5"),
        "font" => (
            "bundled faces are read-only in this build; `kin fonts` lists them, and adding faces lands in P4",
            "P4",
        ),
        "palette from" => (
            "extracting a palette from an image needs the media crate (P4); `kin palette --accent` builds and proves one now",
            "P4",
        ),
        "skills" | "library" | "save" | "judge" => {
            ("the taste, template and judging layer lands in P4", "P4")
        }
        "mix" | "synth" => ("the audio mix and synth verbs land in P5", "P5"),
        _ => ("run `kin capabilities` to see what this build can do", "P0"),
    };
    json!({
        "ok": false,
        "error": {
            "code": "not-implemented",
            "message": format!("`kin {verb}` is not in this build"),
            "hint": hint,
            "planned": phase,
            "fix_op": null,
            "doc": "docs/PLAN.md#plan",
            "verbs": ["capabilities", "fonts", "schema", "new", "probe", "apply", "render", "check", "credits", "log", "diff"],
        }
    })
}

fn capabilities() -> Value {
    json!({
        "ok": true,
        "kin": VERSION,
        "schema": kinema_doc::model::SCHEMA_TAG,
        "tiers": {
            "strict": { "available": true, "engine": "cpu", "guarantee": "bit-identical across machines" },
            "gpu": { "available": false, "hint": "wgpu compositor lands in P1" },
            "fast": { "available": false, "hint": "needs the GPU tier first" }
        },
        "verbs": [
            {"name": "capabilities", "summary": "what this build can do, machine-readable"},
            {"name": "schema", "summary": "JSON Schema for the document"},
            {"name": "new", "summary": "create a project: doc.json plus a journal", "args": ["<dir>", "--force"]},
            {"name": "probe", "summary": "resolved layers at a time", "args": ["--at <seconds>", "--comp <id>", "--layer <id>"]},
            {"name": "eval", "summary": "evaluate one expression against the document", "args": ["<expr>", "--at <seconds>", "--project <dir>"]},
            {"name": "apply", "summary": "apply typed ops, returning their inverses", "args": ["--ops <file|->", "--expect-version <n>", "--dry-run"]},
            {"name": "render", "summary": "render one frame to PNG, or a sequence with a hash manifest", "args": ["--frame <n>", "--frames all|a-b", "-o <file.png|dir>", "--scale <f>", "--comp <id>"]},
            {"name": "check", "summary": "run craft detectors", "args": ["--comp <id>", "--frame <n>", "--fix", "--max-steps <n>"]},
            {"name": "credits", "summary": "licence roll-up and release gate", "args": []},
            {"name": "asset add", "summary": "record an asset, its digest and its licence", "args": ["<file>", "--spdx <id>", "--source-url <url>", "--author <name>", "--kind <kind>", "--id <id>"]},
            {"name": "undo", "summary": "replay the journal's inverses", "args": ["--steps <n>", "--dry-run"]},
            {"name": "verify", "summary": "check a sequence manifest against disk, and with --render against a fresh render", "args": ["--manifest <file>", "--render"]},
            {"name": "diff", "summary": "what changed since a document, and which frames it invalidates", "args": ["--from <doc.json>", "--comp <id>"]},
            {"name": "fonts", "summary": "bundled families, weights, licence and attribution", "args": []},
            {"name": "type", "summary": "the modular scale with optical tracking and leading", "args": ["--base <px>", "--ratio <f>", "--steps <n>"]},
            {"name": "typeface", "summary": "pick a pairing from a brief, with reasons and a role assignment", "args": ["recommend <brief>"]},
            {"name": "palette", "summary": "roles and contrast proofs from one accent", "args": ["--accent <#rrggbb>", "--scheme dark|light"]},
            {"name": "log", "summary": "the op journal", "args": []}
        ],
        "licences": ["cc0-1.0", "cc-by-4.0", "cc-by-sa-4.0", "cc-by-nc-4.0", "cc-by-nd-4.0", "mit", "apache-2.0", "bsd-3-clause", "ofl-1.1", "unknown"],
        "ops": kinema_doc::ops::op_schema(),
        "checks": [
            {"code": "TYPE-FAKE", "severity": "error", "detects": "synthesised italic/bold/small caps, unbundled family", "fix": true},
            {"code": "READ", "severity": "error", "detects": "text under its contrast target against the ground it sits on", "fix": true},
            {"code": "CHROMA", "severity": "warn", "detects": "saturated text under 32 px on a saturated ground", "fix": true},
            {"code": "PALETTE", "severity": "warn", "detects": "more than one accent hue", "fix": true},
            {"code": "EASE-FLAT", "severity": "warn", "detects": "linear or symmetric easing on moves over 200 ms or 2% of frame", "fix": true},
            {"code": "SHUTTER", "severity": "warn", "detects": "motion above 1.5 px/frame with no shutter", "fix": true},
            {"code": "HOLD-NONE", "severity": "warn", "detects": "no still moment in over three seconds", "fix": false},
            {"code": "STAGGER", "severity": "warn", "detects": "more than 70% of layers entering on one frame", "fix": true},
            {"code": "TYPE-SCALE", "severity": "warn", "detects": "a size off the modular scale, or more than four sizes", "fix": true}
        ],
        "checks_advisory": {"HOLD-NONE": "the remedy is which beat to hold; the hint names it, the engine will not pick"},
        "checks_pending": ["CUT-TIMING", "JUMPCUT", "BANDING", "LOUD"],
        "motion_blur": {
            "setting": "comp:<id>/shutter, degrees; 0 disables",
            "film_default": 180,
            "samples": "comp:<id>/samples, 2-64 (default 8)",
            "cost": "linear in samples",
        },
        "layer_kinds": ["solid", "text", "shape"],
        "layer_kinds_pending": ["image", "comp", "adjustment"],
        "props": ["value", "kf", "expr", "link", "graph"],
        "props_working": ["value", "kf", "link", "expr"],
        "expressions": {
            "constants": ["t", "frame", "fps", "width", "height", "duration", "PI", "TAU", "E"],
            "functions": kinema_expr::FUNCTIONS.iter().map(|(n, d)| json!({"name": n, "usage": d})).collect::<Vec<_>>(),
            "bounded": ["no loops, no assignment, no I/O", "4096 byte source", "32 deep", "20000 steps"],
        },
        "fonts": fonts_catalogue(),
        "path_syntax": "comp:main/layer:title/transform.position",
        "projection": "every verb accepts --fields a,b to keep only those top-level keys",
        "error_contract": {"code": "string", "message": "string", "hint": "string", "fix_op": "op|null", "doc": "string"},
        "stateless": "every verb reads the project and writes it back; `--expect-version` gives compare-and-swap for parallel agents",
    })
}

/// The bundle as data: what a document may name, what each family ships, and where its
/// licence text lives.
fn cmd_palette(argv: &[String]) -> R {
    let spec = get(argv, "--accent").unwrap_or("#3b82f6");
    let accent = kinema_doc::Rgba::parse(spec).map_err(|e| {
        json!({
            "ok": false,
            "error": {
                "code": "bad-colour",
                "message": e,
                "hint": "pass --accent as #rgb or #rrggbb, e.g. --accent '#3b82f6'",
                "fix_op": null,
                "doc": "docs/PLAN.md#type-and-colour"
            }
        })
    })?;
    let scheme = get(argv, "--scheme").unwrap_or("dark");
    if scheme != "dark" && scheme != "light" {
        return Err(usage("--scheme is dark or light"));
    }
    let tokens = kinema_palette::scheme(accent, scheme == "dark");
    let proofs: Vec<Value> = tokens
        .proofs()
        .iter()
        .map(|p| {
            json!({
                "pair": format!("{} on {}", p.fg, p.bg),
                "ratio": p.ratio,
                "target": p.target,
                "pass": p.pass,
            })
        })
        .collect();
    let roles: Vec<Value> = tokens
        .roles()
        .iter()
        .map(|(name, colour)| json!({ "role": name, "value": colour.to_hex() }))
        .collect();
    Ok(json!({
        "ok": true,
        "scheme": scheme,
        "accent": accent.to_hex(),
        "tokens": roles,
        "proofs": proofs,
        "clean": proofs.iter().all(|p| p["pass"] == json!(true)),
        "rules": [
            "one accent hue, neutrals tinted with it, no rainbow gradient",
            "text is proven on both the ground and the raised surface",
            "accent_ink is proven on the accent itself, so a button label always reads",
            "PALETTE reports a document that reaches for a second accent hue"
        ],
    }))
}

fn cmd_type(argv: &[String]) -> Value {
    let num = |flag: &str, default: f64| {
        get(argv, flag)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default)
    };
    let base = num("--base", kinema_type::HOUSE_BASE);
    let ratio = num("--ratio", kinema_type::HOUSE_RATIO);
    let steps = get(argv, "--steps")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| (2..=16).contains(n))
        .unwrap_or(kinema_type::HOUSE_STEPS);
    let ladder = kinema_type::ladder(base, ratio, steps);
    json!({
        "ok": true,
        "base": base,
        "ratio": ratio,
        "ladder": ladder
            .iter()
            .map(|s| json!({
                "index": s.index,
                "role": kinema_type::role_of(s.index),
                "size": s.size,
                "tracking": s.tracking,
                "leading": s.leading,
            }))
            .collect::<Vec<_>>(),
        "rules": [
            "every text size should be a step here; TYPE-SCALE reports the ones that are not",
            "tracking is optical: display tightens, small text opens up",
            "leading follows the same curve: 1.5 at 16 px down to 1.1 at 96 px",
            "at most four sizes per comp; a fifth reads as a slide deck"
        ],
    })
}

fn cmd_typeface(argv: &[String]) -> Value {
    let pos = positionals(argv);
    let brief = pos.iter().skip(2).cloned().collect::<Vec<_>>().join(" ");
    let (pairing, matched) = kinema_type::recommend(&brief);
    let ladder = kinema_type::house_ladder();
    let role = |name: &str, index: usize| {
        let s = ladder[index.min(ladder.len() - 1)];
        json!({
            "role": name,
            "family": kinema_type::family_for(pairing, name),
            "size": s.size,
            "tracking": s.tracking,
            "leading": s.leading,
        })
    };
    json!({
        "ok": true,
        "brief": brief,
        "pairing": {
            "id": pairing.id,
            "display": pairing.display,
            "body": pairing.body,
            "mono": pairing.mono,
            "why": pairing.why,
        },
        "matched_keywords": matched,
        "assignment": [
            role("title", 4),
            role("body", 1),
            role("mono", 1),
            role("caption", 0),
        ],
        "pairing_catalogue": kinema_type::PAIRINGS.iter().map(|p| json!({
            "id": p.id, "display": p.display, "body": p.body, "mono": p.mono, "use_when": p.use_when
        })).collect::<Vec<_>>(),
        "apply_with": "set text.font, text.size, text.tracking and text.line_height from the assignment; `kin apply` takes them as ops",
    })
}

fn fonts_catalogue() -> Value {
    let families: Vec<Value> = kinema_text::families()
        .iter()
        .map(|family| {
            let faces: Vec<Value> = kinema_text::FACES
                .iter()
                .filter(|f| f.family == *family)
                .map(|f| {
                    json!({
                        "weight": f.weight,
                        "italic": f.italic,
                        "file": f.file,
                        "spdx": f.spdx,
                        "author": f.author,
                        "licence_file": f.licence_file,
                    })
                })
                .collect();
            json!({
                "family": family,
                "weights": kinema_text::weights(family),
                "italic": kinema_text::has_italic(family),
                "faces": faces,
            })
        })
        .collect();
    json!({
        "families": families,
        "default": kinema_text::DEFAULT_FAMILY,
        "bundled": true,
        "note": "names are exact; anything else falls back and TYPE-FAKE reports it",
    })
}

fn cmd_fonts() -> Value {
    json!({
        "ok": true,
        "fonts": fonts_catalogue(),
        "advice": [
            "pair a grotesque with a mono, and let weight carry hierarchy",
            "there is no italic for the mono: emphasis there is weight or size, never a slant",
            "the faces are OFL, so `kin credits` already includes their attribution"
        ],
    })
}

fn help() -> Value {
    json!({
        "ok": true,
        "usage": "kin <verb> [args] [--project <dir>] [--pretty]",
        "verbs": ["capabilities", "fonts", "schema", "new", "probe", "apply", "render", "check", "credits", "log", "diff"],
        "examples": [
            "kin new demo",
            "kin render --project demo --frame 0 -o demo/frame0.png",
            "kin probe --project demo --at 1.5",
            "kin check --project demo"
        ]
    })
}

// ---------------------------------------------------------------- project

struct Project {
    dir: PathBuf,
}

impl Project {
    fn at(dir: PathBuf) -> Self {
        Project { dir }
    }

    fn open(argv: &[String]) -> Result<Self, Value> {
        let dir = get(argv, "--project").unwrap_or(".").to_string();
        let p = Project::at(PathBuf::from(&dir));
        if !p.doc_path().exists() {
            return Err(json!({
                "ok": false,
                "error": {
                    "code": "no-project",
                    "message": format!("no doc.json in `{}`", p.dir.display()),
                    "hint": "run `kin new <dir>` first, or pass --project",
                    "fix_op": null,
                    "doc": "docs/PLAN.md#contract"
                }
            }));
        }
        Ok(p)
    }

    fn doc_path(&self) -> PathBuf {
        self.dir.join("doc.json")
    }

    fn journal_path(&self) -> PathBuf {
        self.dir.join(".kinema").join("journal.jsonl")
    }

    fn load(&self) -> Result<Doc, Value> {
        let src = fs::read_to_string(self.doc_path()).map_err(|e| io_err(&self.doc_path(), &e))?;
        Doc::parse(&src).map_err(|e| doc_err(&e))
    }

    /// Atomic: a crash mid-write never leaves a half document behind.
    fn save(&self, doc: &Doc) -> Result<(), Value> {
        let body = doc.to_pretty().map_err(|e| doc_err(&e))?;
        let tmp = self.doc_path().with_extension("json.tmp");
        fs::write(&tmp, body.as_bytes()).map_err(|e| io_err(&tmp, &e))?;
        fs::rename(&tmp, self.doc_path()).map_err(|e| io_err(&self.doc_path(), &e))
    }

    fn append_journal(&self, entry: &Value) -> Result<(), Value> {
        let path = self.journal_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, &e))?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| io_err(&path, &e))?;
        writeln!(f, "{}", serde_json::to_string(entry).unwrap_or_default())
            .map_err(|e| io_err(&path, &e))
    }
}

fn io_err(path: &Path, e: &std::io::Error) -> Value {
    json!({
        "ok": false,
        "error": {
            "code": "io",
            "message": format!("{}: {e}", path.display()),
            "hint": "check the path exists and is writable",
            "fix_op": null,
            "doc": "docs/PLAN.md#contract"
        }
    })
}

fn doc_err(e: &kinema_doc::DocError) -> Value {
    let mut err = e.to_json();
    err["ok"] = json!(false);
    json!({ "ok": false, "error": err, "code": err.get("code") })
}

fn cmd_new(argv: &[String]) -> R {
    let pos = positionals(argv);
    // `pos[0]` is the verb itself; the directory is the first real argument.
    let dir = PathBuf::from(pos.get(1).cloned().unwrap_or_else(|| ".".to_string()));
    let doc_path = dir.join("doc.json");
    if doc_path.exists() && !has(argv, "--force") {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "exists",
                "message": format!("`{}` already exists", doc_path.display()),
                "hint": "pass --force to overwrite, or pick another directory",
                "fix_op": null,
                "doc": "docs/PLAN.md#contract"
            }
        }));
    }
    let project = Project::at(dir.clone());
    fs::create_dir_all(&dir).map_err(|e| io_err(&dir, &e))?;
    fs::create_dir_all(dir.join("assets")).map_err(|e| io_err(&dir, &e))?;
    let doc = Doc::starter();
    project.save(&doc)?;
    project.append_journal(&json!({
        "seq": 0,
        "at": now(),
        "actor": actor(),
        "op": "new",
        "reason": "created from the starter document",
        "doc_after": deps::hash_doc(&doc),
    }))?;
    Ok(json!({
        "ok": true,
        "project": dir.display().to_string(),
        "doc": doc_path.display().to_string(),
        "version": doc.version,
        "doc_hash": deps::hash_doc(&doc),
        "comp": "main",
        "next": [
            "kin render --project <dir> --frame 0 -o frame0.png",
            "kin apply --project <dir> --ops ops.json",
            "kin check --project <dir> --fix"
        ]
    }))
}

fn cmd_probe(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let comp_id = get(argv, "--comp").unwrap_or("main").to_string();
    let at = get(argv, "--at")
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|_| usage("--at expects seconds, e.g. --at 1.5"))?
        .unwrap_or(0.0);
    let only = get(argv, "--layer").map(str::to_string);
    let comp = doc.comp(&comp_id).map_err(|e| doc_err(&e))?;
    let frame = doc.fps.ticks_to_frame(kinema_time::seconds_to_ticks(at));
    let ev = doc.eval_comp(&comp_id, at).map_err(|e| doc_err(&e))?;
    let layers: Vec<Value> = comp
        .layers
        .iter()
        .zip(ev.iter())
        .filter(|(l, _)| only.as_ref().map(|o| &l.id == o).unwrap_or(true))
        .map(|(l, e)| {
            json!({
                "id": l.id,
                "kind": l.kind.as_str(),
                "start": l.start,
                "duration": l.duration,
                "visible": e.visible,
                "opacity": (e.opacity * 1000.0).round() / 1000.0,
                "bbox": e.bbox.map(|v| (v * 100.0).round() / 100.0),
                "position": [e.position[0], e.position[1]],
                "scale": e.scale,
                "rotation": e.rotation,
                "text": e.text,
                "font": e.font,
                "opacity_prop": l.opacity.kind(),
            })
        })
        .collect();
    Ok(json!({
        "ok": true,
        "version": doc.version,
        "doc_hash": deps::hash_doc(&doc),
        "fps": [doc.fps.0.num, doc.fps.0.den],
        "time": at,
        "frame": frame,
        "timecode": kinema_time::timecode(kinema_time::seconds_to_ticks(at), doc.fps),
        "comp": {
            "id": comp_id,
            "size": comp.size,
            "duration": comp.duration,
            "frames": doc.frame_count(&comp_id).map_err(|e| doc_err(&e))?,
        },
        "layers": layers,
    }))
}

fn cmd_eval(argv: &[String]) -> R {
    let pos = positionals(argv);
    let src = get(argv, "--expr")
        .map(str::to_string)
        .or_else(|| pos.get(1).cloned());
    let Some(src) = src else {
        return Err(usage(
            "pass an expression: kin eval 'clamp(t, 0, 1)' --at 1.5",
        ));
    };
    let doc = if get(argv, "--project").is_some() {
        Project::open(argv)?.load()?
    } else {
        Doc::starter()
    };
    let t: f64 = get(argv, "--at")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| usage("--at expects seconds, e.g. --at 1.5"))?
        .unwrap_or(0.0);
    let value = kinema_doc::prop::eval_source(&doc, &src, t, 0)
        .map_err(|e| json!({ "ok": false, "error": e.to_json() }))?;
    Ok(json!({
        "ok": true,
        "expr": src,
        "at": t,
        "frame": doc.fps.ticks_to_frame(kinema_time::seconds_to_ticks(t)),
        "value": value,
    }))
}

fn cmd_apply(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let mut doc = project.load()?;
    if let Some(v) = get(argv, "--expect-version") {
        let expect: u64 = v
            .parse()
            .map_err(|_| usage("--expect-version expects an integer"))?;
        if expect != doc.version {
            return Err(json!({
                "ok": false,
                "error": {
                    "code": "version-conflict",
                    "message": format!("document is at version {}, you expected {}", doc.version, expect),
                    "hint": "rebase: reload with `kin probe`, re-read the value, and retry with the current version",
                    "fix_op": null,
                    "doc": "docs/PLAN.md#contract",
                    "current_version": doc.version
                }
            }));
        }
    }
    let src = match get(argv, "--ops") {
        Some(path) if path != "-" => {
            fs::read_to_string(path).map_err(|e| io_err(Path::new(path), &e))?
        }
        _ => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| json!({ "ok": false, "error": { "code": "io", "message": e.to_string(), "hint": "pipe ops as JSON on stdin", "fix_op": null, "doc": "docs/PLAN.md#contract" } }))?;
            buf
        }
    };
    let parsed: Value = serde_json::from_str(&src).map_err(|e| {
        json!({
            "ok": false,
            "error": {
                "code": "invalid-json",
                "message": e.to_string(),
                "hint": "ops are a JSON array, or one op object",
                "fix_op": null,
                "doc": "docs/PLAN.md#contract"
            }
        })
    })?;
    let ops: Vec<Op> = match &parsed {
        Value::Array(_) => serde_json::from_value(parsed.clone()).map_err(|e| op_err(&e))?,
        Value::Object(_) => vec![serde_json::from_value(parsed.clone()).map_err(|e| op_err(&e))?],
        _ => return Err(usage("ops must be an array or a single op object")),
    };

    let before = deps::hash_doc(&doc);
    let mut applied = Vec::new();
    for op in &ops {
        let res = doc.apply(op).map_err(|e| doc_err(&e))?;
        applied.push(json!({
            "op": res.op,
            "version": res.version,
            "inverse": res.inverse,
            "reason": op.reason(),
        }));
        if !has(argv, "--dry-run") {
            project.append_journal(&json!({
                "seq": doc.version,
                "at": now(),
                "actor": actor(),
                "op": op,
                "reason": op.reason(),
                "inverse": res.inverse,
                "doc_before": before,
                "doc_after": deps::hash_doc(&doc),
            }))?;
        }
    }
    if !has(argv, "--dry-run") {
        project.save(&doc)?;
    }
    let problems = doc.validate();
    Ok(json!({
        "ok": problems.is_empty(),
        "dry_run": has(argv, "--dry-run"),
        "version": doc.version,
        "doc_hash": deps::hash_doc(&doc),
        "applied": applied,
        "problems": problems,
    }))
}

fn op_err(e: &serde_json::Error) -> Value {
    json!({
        "ok": false,
        "error": {
            "code": "invalid-op",
            "message": e.to_string(),
            "hint": "ops are kebab-case tagged: set-prop, insert-layer, delete-layer, set-comp, set-asset",
            "fix_op": null,
            "doc": "docs/PLAN.md#contract",
            "schema": "kin capabilities | jq .ops"
        }
    })
}

fn cmd_render(argv: &[String]) -> R {
    if let Some(spec) = get(argv, "--frames") {
        return render_sequence(argv, spec);
    }
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let comp = get(argv, "--comp").unwrap_or("main").to_string();
    let frame: u32 = get(argv, "--frame")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| usage("--frame expects an integer"))?
        .unwrap_or(0);
    let scale: f64 = get(argv, "--scale")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| usage("--scale expects a number, e.g. --scale 0.5"))?
        .unwrap_or(1.0);
    let out = get(argv, "-o")
        .or_else(|| get(argv, "--out"))
        .unwrap_or("frame.png")
        .to_string();

    let frames = doc.frame_count(&comp).map_err(|e| doc_err(&e))?;
    if frames > 0 && frame >= frames {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "out-of-range",
                "message": format!("frame {frame} is past the last frame ({})", frames - 1),
                "hint": format!("`{comp}` is {} frames at {} fps", frames, doc.fps.as_f64()),
                "fix_op": {"op": "set-comp", "comp": comp, "duration": kinema_time::ticks_to_seconds(doc.fps.frame_to_ticks(frame + 1)) as f64},
                "doc": "docs/PLAN.md#contract"
            }
        }));
    }

    let started = Instant::now();
    let rendered = raster::render_frame(&doc, &comp, frame, scale)
        .map_err(|e| json!({ "ok": false, "error": e.to_json() }))?;
    let path = PathBuf::from(&out);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, &e))?;
        }
    }
    fs::write(&path, &rendered.png).map_err(|e| io_err(&path, &e))?;
    let ms = started.elapsed().as_millis();
    Ok(json!({
        "ok": true,
        "comp": comp,
        "frame": frame,
        "time": doc.frame_time(frame),
        "timecode": kinema_time::timecode(doc.fps.frame_to_ticks(frame), doc.fps),
        "out": out,
        "bytes": rendered.png.len(),
        "size": [rendered.width, rendered.height],
        "scale": scale,
        "sha256": deps::hash_bytes(&rendered.png),
        "frame_hash": deps::hash_frame(&doc, &comp, frame, "strict"),
        "layers": rendered.layers.iter().map(|l| json!({"id": l.id, "kind": l.kind, "opacity": l.opacity})).collect::<Vec<_>>(),
        "samples": rendered.samples,
        "skipped": rendered.skipped,
        "ms": ms,
    }))
}

/// A sequence is a directory of PNGs plus a manifest of hashes: the manifest is what an
/// encoder, a cache and a golden test all read.
fn render_sequence(argv: &[String], spec: &str) -> R {
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let comp = get(argv, "--comp").unwrap_or("main").to_string();
    let scale: f64 = get(argv, "--scale")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| usage("--scale expects a number, e.g. --scale 0.5"))?
        .unwrap_or(1.0);
    let total = doc.frame_count(&comp).map_err(|e| doc_err(&e))?;
    let (start, end): (u32, u32) = if spec == "all" {
        (0, total.saturating_sub(1))
    } else {
        let (a, b) = spec
            .split_once('-')
            .ok_or_else(|| usage("--frames is `all` or `first-last`, e.g. --frames 0-59"))?;
        (
            a.trim()
                .parse()
                .map_err(|_| usage("--frames start must be an integer"))?,
            b.trim()
                .parse()
                .map_err(|_| usage("--frames end must be an integer"))?,
        )
    };
    if start > end {
        return Err(usage("the frame range is backwards"));
    }
    if end >= total {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "out-of-range",
                "message": format!("frame {end} is past the last frame ({})", total.saturating_sub(1)),
                "hint": format!("`{comp}` has {total} frames; use --frames all or a smaller end"),
                "fix_op": null,
                "doc": "docs/PLAN.md#contract",
            }
        }));
    }
    let dir = PathBuf::from(
        get(argv, "-o")
            .or_else(|| get(argv, "--out"))
            .unwrap_or("frames"),
    );
    fs::create_dir_all(&dir).map_err(|e| io_err(&dir, &e))?;
    let size = {
        let c = doc.comp(&comp).map_err(|e| doc_err(&e))?;
        [
            ((c.size[0] as f64) * scale).round().max(1.0) as u32,
            ((c.size[1] as f64) * scale).round().max(1.0) as u32,
        ]
    };

    // Incremental re-render: a frame is reusable when its identity is unchanged and the
    // file on disk is still the file we recorded. Anything else is drawn again.
    let variant = format!("strict@{scale}");
    let mut previous: std::collections::BTreeMap<u32, Value> = std::collections::BTreeMap::new();
    if let Ok(src) = fs::read_to_string(dir.join("manifest.json")) {
        if let Some(frames) = serde_json::from_str::<Value>(&src)
            .ok()
            .and_then(|m| m.get("frames").and_then(|f| f.as_array()).cloned())
        {
            for f in frames {
                if let Some(n) = f.get("frame").and_then(|v| v.as_u64()) {
                    previous.insert(n as u32, f);
                }
            }
        }
    }

    let started = Instant::now();
    let mut frames = Vec::new();
    let mut rendered = 0usize;
    let mut reused = 0usize;
    for frame in start..=end {
        let frame_hash = deps::hash_frame(&doc, &comp, frame, &variant);
        if let Some(prev) = previous.get(&frame) {
            let file = prev
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let sha = prev
                .get("sha256")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let same_identity =
                prev.get("frame_hash").and_then(|v| v.as_str()) == Some(&frame_hash);
            if same_identity && !file.is_empty() {
                if let Ok(bytes) = fs::read(dir.join(file)) {
                    if deps::hash_bytes(&bytes) == sha {
                        frames.push(prev.clone());
                        reused += 1;
                        continue;
                    }
                }
            }
        }
        let out = raster::render_frame(&doc, &comp, frame, scale)
            .map_err(|e| json!({ "ok": false, "error": e.to_json() }))?;
        let file = format!("frame_{frame:05}.png");
        fs::write(dir.join(&file), &out.png).map_err(|e| io_err(&dir.join(&file), &e))?;
        frames.push(json!({
            "frame": frame,
            "file": file,
            "bytes": out.png.len(),
            "sha256": deps::hash_bytes(&out.png),
            "pixels": deps::hash_bytes(&out.pixels),
            "frame_hash": frame_hash,
        }));
        rendered += 1;
    }
    let ms = started.elapsed().as_millis();
    let manifest = json!({
        "schema": "kinema/sequence/1",
        "engine": VERSION,
        "tier": "strict",
        "comp": comp,
        "fps": [doc.fps.0.num, doc.fps.0.den],
        "size": size,
        "doc_hash": deps::hash_doc(&doc),
        "count": frames.len(),
        "first": start,
        "last": end,
        "variant": variant,
        "scale": scale,
        "rendered": rendered,
        "reused": reused,
        "total_ms": ms,
        "frames": frames,
    });
    let manifest_path = dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| io_err(&manifest_path, &e))?;

    let mut summary = manifest.clone();
    summary["frames"] = json!({ "count": manifest["count"] });
    summary["ok"] = json!(true);
    summary["manifest"] = json!(manifest_path.display().to_string());
    Ok(summary)
}

/// Check a sequence manifest against the files on disk, and with `--render`, against a
/// fresh render. The second is the reproducibility claim: same document, same pixels.
/// What a document change actually costs: which layers moved, and how many frames the
/// renderer will have to draw again.
fn cmd_diff(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let after = project.load()?;
    let base_path = get(argv, "--from")
        .ok_or_else(|| usage("pass --from <doc.json>; the working document is the other side"))?;
    let src = fs::read_to_string(base_path).map_err(|e| io_err(Path::new(base_path), &e))?;
    let before = Doc::parse(&src).map_err(|e| doc_err(&e))?;
    let comp = get(argv, "--comp").unwrap_or("main").to_string();

    let mut layers_changed = Vec::new();
    let mut comps_changed = Vec::new();
    for (id, b) in &before.comps {
        match after.comps.get(id) {
            None => comps_changed.push(json!({ "comp": id, "change": "removed" })),
            Some(a) => {
                let mut fields = Vec::new();
                if a.size != b.size {
                    fields.push("size");
                }
                if a.duration != b.duration {
                    fields.push("duration");
                }
                if a.shutter != b.shutter {
                    fields.push("shutter");
                }
                if a.samples != b.samples {
                    fields.push("samples");
                }
                if !fields.is_empty() {
                    comps_changed
                        .push(json!({ "comp": id, "change": "changed", "fields": fields }));
                }
                for bl in &b.layers {
                    match a.layers.iter().find(|l| l.id == bl.id) {
                        None => layers_changed.push(json!({
                            "comp": id, "layer": bl.id, "change": "removed"
                        })),
                        Some(al) => {
                            let fields = differing_fields(
                                &serde_json::to_value(bl).unwrap_or(Value::Null),
                                &serde_json::to_value(al).unwrap_or(Value::Null),
                            );
                            if !fields.is_empty() {
                                layers_changed.push(json!({
                                    "comp": id, "layer": al.id, "change": "changed", "fields": fields
                                }));
                            }
                        }
                    }
                }
                for al in &a.layers {
                    if !b.layers.iter().any(|l| l.id == al.id) {
                        layers_changed.push(json!({
                            "comp": id, "layer": al.id, "change": "added"
                        }));
                    }
                }
            }
        }
    }
    for id in after.comps.keys() {
        if !before.comps.contains_key(id) {
            comps_changed.push(json!({ "comp": id, "change": "added" }));
        }
    }

    let total = after.frame_count(&comp).unwrap_or(0);
    let invalidated = (0..total)
        .filter(|f| {
            deps::hash_frame(&before, &comp, *f, "diff")
                != deps::hash_frame(&after, &comp, *f, "diff")
        })
        .count();

    let assets_changed: Vec<Value> = after
        .assets
        .iter()
        .filter(|(id, a)| before.assets.get(*id).map(|b| b != *a).unwrap_or(true))
        .map(|(id, _)| json!({ "asset": id, "change": "changed" }))
        .chain(
            before
                .assets
                .keys()
                .filter(|id| !after.assets.contains_key(*id))
                .map(|id| json!({ "asset": id, "change": "removed" })),
        )
        .collect();

    Ok(json!({
        "ok": true,
        "comp": comp,
        "doc_hash_before": deps::hash_doc(&before),
        "doc_hash_after": deps::hash_doc(&after),
        "same": deps::hash_doc(&before) == deps::hash_doc(&after),
        "comps": comps_changed,
        "layers": layers_changed,
        "assets": assets_changed,
        "frames_total": total,
        "frames_invalidated": invalidated,
        "frames_reusable": total as usize - invalidated,
    }))
}

/// Which top-level (and one level of nested) keys differ between two values.
fn differing_fields(a: &Value, b: &Value) -> Vec<String> {
    let (Some(x), Some(y)) = (a.as_object(), b.as_object()) else {
        return if a == b {
            Vec::new()
        } else {
            vec!["value".into()]
        };
    };
    let mut out = Vec::new();
    for (k, av) in x {
        match y.get(k) {
            None => out.push(k.clone()),
            Some(bv) if av == bv => {}
            Some(bv) => match (av.as_object(), bv.as_object()) {
                (Some(ao), Some(bo)) => {
                    for (ik, iv) in ao {
                        if bo.get(ik) != Some(iv) {
                            out.push(format!("{k}.{ik}"));
                        }
                    }
                    for ik in bo.keys() {
                        if !ao.contains_key(ik) {
                            out.push(format!("{k}.{ik}"));
                        }
                    }
                }
                _ => out.push(k.clone()),
            },
        }
    }
    for k in y.keys() {
        if !x.contains_key(k) {
            out.push(k.clone());
        }
    }
    out.sort();
    out
}

fn cmd_verify(argv: &[String]) -> R {
    let manifest_path =
        get(argv, "--manifest").ok_or_else(|| usage("pass --manifest <dir>/manifest.json"))?;
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let src =
        fs::read_to_string(manifest_path).map_err(|e| io_err(Path::new(manifest_path), &e))?;
    let manifest: Value = serde_json::from_str(&src).map_err(|e| {
        json!({
            "ok": false,
            "error": {
                "code": "invalid-json",
                "message": e.to_string(),
                "hint": "--manifest takes a manifest.json written by `kin render --frames`",
                "fix_op": null,
                "doc": "docs/PLAN.md#contract",
            }
        })
    })?;
    let comp = manifest["comp"].as_str().unwrap_or("main").to_string();
    let scale = manifest["scale"]
        .as_f64()
        .or_else(|| {
            manifest["variant"]
                .as_str()
                .and_then(|v| v.split('@').nth(1))
                .and_then(|s| s.parse::<f64>().ok())
        })
        .unwrap_or(1.0);
    let entries = manifest["frames"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        return Err(usage("the manifest lists no frames"));
    }
    let re_render = has(argv, "--render");
    let doc_hash = deps::hash_doc(&doc);
    let doc_changed = manifest["doc_hash"].as_str() != Some(doc_hash.as_str());
    let dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));

    let mut checked = 0usize;
    let mut mismatched = Vec::new();
    let mut missing = Vec::new();
    for entry in &entries {
        let frame = entry["frame"].as_u64().unwrap_or(0) as u32;
        let file = entry["file"].as_str().unwrap_or_default();
        let path = dir.join(file);
        if file.is_empty() || !path.exists() {
            missing.push(json!({ "frame": frame, "file": file }));
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| io_err(&path, &e))?;
        if deps::hash_bytes(&bytes) != entry["sha256"].as_str().unwrap_or_default() {
            mismatched.push(json!({ "frame": frame, "reason": "file", "file": file }));
            continue;
        }
        if re_render {
            let out = raster::render_frame(&doc, &comp, frame, scale)
                .map_err(|e| json!({ "ok": false, "error": e.to_json() }))?;
            let actual = deps::hash_bytes(&out.pixels);
            if actual != entry["pixels"].as_str().unwrap_or_default() {
                mismatched.push(json!({
                    "frame": frame,
                    "reason": "render",
                    "expected": entry["pixels"],
                    "actual": actual,
                }));
                continue;
            }
        }
        checked += 1;
    }
    let ok = mismatched.is_empty() && missing.is_empty();
    Ok(json!({
        "ok": ok,
        "manifest": manifest_path,
        "comp": comp,
        "checked": checked,
        "mismatched": mismatched,
        "missing": missing,
        "re_rendered": re_render,
        "doc_changed": doc_changed,
        "note": if doc_changed && re_render {
            json!("the document has changed since this sequence was rendered, so render mismatches are expected")
        } else {
            Value::Null
        },
    }))
}

fn cmd_check(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let mut doc = project.load()?;
    let comp = get(argv, "--comp").unwrap_or("main").to_string();
    let frame = get(argv, "--frame").and_then(|s| s.parse::<u32>().ok());

    // Fixes are computed against one snapshot, so a batch of them can fight: the remedy
    // for CHROMA is not the remedy for READ. The fixpoint belongs to the checks crate.
    let mut fixes = Vec::new();
    let mut conflict = Value::Null;
    if has(argv, "--fix") {
        let max: usize = get(argv, "--max-steps")
            .map(|s| s.parse())
            .transpose()
            .map_err(|_| usage("--max-steps expects an integer"))?
            .unwrap_or(20);
        let report = checks::fixpoint(&doc, &comp, frame, max).map_err(|e| doc_err(&e))?;
        for step in &report.steps {
            project.append_journal(&json!({
                "seq": step.version,
                "at": now(),
                "actor": actor(),
                "op": step.op,
                "reason": format!("fix for {}", step.code),
                "doc_after": deps::hash_doc(&report.doc),
            }))?;
            fixes.push(json!({
                "code": step.code,
                "layer": step.layer,
                "op": step.op,
                "version": step.version,
            }));
        }
        if let Some(message) = &report.conflict {
            conflict = json!(message);
        }
        if !report.steps.is_empty() {
            project.save(&report.doc)?;
            doc = report.doc;
        }
    }

    let found = checks::run(&doc, &comp, frame);
    let mut counts = serde_json::Map::new();
    for d in &found {
        let n = counts.get(d.code).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(d.code.to_string(), json!(n + 1));
    }
    let errors = found.iter().filter(|d| d.severity == "error").count();
    let warnings = found.len() - errors;
    Ok(json!({
        "ok": errors == 0,
        "version": doc.version,
        "doc_hash": deps::hash_doc(&doc),
        "comp": comp,
        "errors": errors,
        "warnings": warnings,
        "counts": counts,
        "fixed": if has(argv, "--fix") { json!(fixes) } else { Value::Null },
        "fix_conflict": conflict,
        "defects": found.iter().map(|d| d.to_json()).collect::<Vec<_>>(),
        "note": if found.is_empty() { Value::Null } else { json!("every defect carries a fix_op you can apply with `kin apply`") },
    }))
}

fn cmd_credits(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let mut entries = Vec::new();
    for (id, asset) in &doc.assets {
        let credit = asset.licence.as_ref().and_then(|l| l.credit_line(asset));
        let known = asset
            .licence
            .as_ref()
            .map(|l| l.is_known())
            .unwrap_or(false);
        let commercial = asset
            .licence
            .as_ref()
            .map(|l| l.commercial_ok)
            .unwrap_or(false);
        entries.push(json!({
            "id": id,
            "kind": asset.kind,
            "path": asset.path,
            "spdx": asset.licence.as_ref().map(|l| l.spdx.clone()),
            "credit": credit,
            "release_ok": known && commercial,
            "problem": match (&asset.licence, known, commercial) {
                (None, _, _) => Some("no licence datablock"),
                (Some(_), false, _) => Some("licence is unknown; it was not guessed for you"),
                (Some(_), true, false) => Some("licence forbids commercial use"),
                _ => None,
            },
        }));
    }
    let release_ok = entries.iter().all(|e| e["release_ok"] == json!(true));
    Ok(json!({
        "ok": true,
        "assets": entries,
        "release_ok": release_ok,
        "note": "a release render refuses unknown licences and missing credits",
    }))
}

fn cmd_undo(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let mut doc = project.load()?;
    let steps: usize = get(argv, "--steps")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| usage("--steps expects an integer"))?
        .unwrap_or(1);
    let path = project.journal_path();
    let src = fs::read_to_string(&path).unwrap_or_default();
    let entries: Vec<Value> = src
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let inverses: Vec<Value> = entries
        .iter()
        .rev()
        .filter_map(|e| e.get("inverse").cloned())
        .filter(|v| !v.is_null())
        .collect();
    if inverses.is_empty() {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "nothing-to-undo",
                "message": "the journal holds no inverses",
                "hint": "only applied ops are invertible; `new` and journal-less edits are not",
                "fix_op": null,
                "doc": "docs/PLAN.md#contract",
            }
        }));
    }
    let mut undone = Vec::new();
    for inv in inverses.iter().take(steps.min(inverses.len())) {
        let op: Op = serde_json::from_value(inv.clone()).map_err(|e| op_err(&e))?;
        let before = deps::hash_doc(&doc);
        let res = doc.apply(&op).map_err(|e| doc_err(&e))?;
        undone.push(json!({
            "op": op,
            "version": res.version,
            "inverse": res.inverse,
        }));
        if !has(argv, "--dry-run") {
            project.append_journal(&json!({
                "seq": doc.version,
                "at": now(),
                "actor": actor(),
                "op": op,
                "reason": "undo",
                "inverse": res.inverse,
                "doc_before": before,
                "doc_after": deps::hash_doc(&doc),
            }))?;
        }
    }
    if !has(argv, "--dry-run") {
        project.save(&doc)?;
    }
    Ok(json!({
        "ok": true,
        "dry_run": has(argv, "--dry-run"),
        "undone": undone,
        "version": doc.version,
        "doc_hash": deps::hash_doc(&doc),
    }))
}

/// Licences are looked up, never guessed. An unrecognised SPDX string is a refusal, not
/// a licence with everything set to false.
fn licence_flags(spdx: &str) -> Option<(bool, bool, bool, bool)> {
    let key = spdx.trim().to_ascii_lowercase();
    let flags = match key.as_str() {
        "cc0-1.0" | "cc0" => (false, true, true, false),
        "cc-by-4.0" | "cc-by-3.0" => (true, true, true, false),
        "cc-by-sa-4.0" | "cc-by-sa-3.0" => (true, true, true, true),
        "cc-by-nc-4.0" | "cc-by-nc-3.0" => (true, false, true, false),
        "cc-by-nd-4.0" => (true, true, false, false),
        "mit" | "apache-2.0" | "bsd-3-clause" => (true, true, true, false),
        "ofl-1.1" | "ofl-1.0" => (true, true, true, false),
        "unknown" => (false, false, false, false),
        _ => return None,
    };
    Some(flags)
}

fn kind_from_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff" | "exr" => "image",
        "wav" | "mp3" | "flac" | "ogg" | "aif" | "aiff" => "audio",
        "mp4" | "mov" | "mkv" | "webm" => "video",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        "glb" | "gltf" | "obj" | "fbx" => "model",
        "hdr" | "hdri" => "hdri",
        _ => "image",
    }
}

fn cmd_asset(argv: &[String]) -> R {
    let pos = positionals(argv);
    let sub = pos.get(1).map(String::as_str).unwrap_or("");
    if sub != "add" {
        return Err(not_implemented("assets"));
    }
    let Some(file) = pos.get(2) else {
        return Err(usage(
            "kin asset add <file> --spdx CC0-1.0 --source-url <url> --author <name>",
        ));
    };
    let project = Project::open(argv)?;
    let mut doc = project.load()?;
    let path = PathBuf::from(file);
    let bytes = fs::read(&path).map_err(|e| io_err(&path, &e))?;
    let spdx = get(argv, "--spdx").unwrap_or("unknown").to_string();
    let Some((attribution, commercial, derivatives, share_alike)) = licence_flags(&spdx) else {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "unknown-licence",
                "message": format!("`{spdx}` is not a licence this build can describe"),
                "hint": "licences are looked up, not guessed: use cc0-1.0, cc-by-4.0, cc-by-sa-4.0, cc-by-nc-4.0, cc-by-nd-4.0, mit, apache-2.0, bsd-3-clause, ofl-1.1, or `unknown`",
                "fix_op": null,
                "doc": "docs/PLAN.md#assets",
                "offered": spdx,
            }
        }));
    };
    let source_url = get(argv, "--source-url").unwrap_or("").to_string();
    if attribution && source_url.is_empty() {
        return Err(json!({
            "ok": false,
            "error": {
                "code": "credit-incomplete",
                "message": format!("`{spdx}` requires attribution, so a source URL is required"),
                "hint": "pass --source-url; the credit line is emitted by `kin credits`",
                "fix_op": null,
                "doc": "docs/PLAN.md#assets",
            }
        }));
    }
    let id = get(argv, "--id").map(str::to_string).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("asset")
            .to_string()
    });
    let asset = kinema_doc::Asset {
        id: id.clone(),
        kind: get(argv, "--kind")
            .unwrap_or(kind_from_path(&path))
            .to_string(),
        path: file.to_string(),
        digest: deps::hash_bytes(&bytes),
        licence: Some(kinema_doc::Licence {
            spdx: spdx.clone(),
            source_url,
            author: get(argv, "--author").unwrap_or("").to_string(),
            retrieved_at: get(argv, "--retrieved")
                .map(str::to_string)
                .unwrap_or_else(today),
            attribution_required: attribution,
            commercial_ok: commercial,
            derivatives_ok: derivatives,
            share_alike,
            model_release: has(argv, "--model-release"),
        }),
    };
    let op = Op::SetAsset {
        asset: asset.clone(),
        reason: "record an asset and its licence".into(),
    };
    let res = doc.apply(&op).map_err(|e| doc_err(&e))?;
    project.append_journal(&json!({
        "seq": doc.version,
        "at": now(),
        "actor": actor(),
        "op": op,
        "reason": "record an asset and its licence",
        "inverse": res.inverse,
        "doc_after": deps::hash_doc(&doc),
    }))?;
    project.save(&doc)?;
    let credit = asset.licence.as_ref().and_then(|l| l.credit_line(&asset));
    Ok(json!({
        "ok": true,
        "asset": {
            "id": id,
            "kind": asset.kind,
            "bytes": bytes.len(),
            "digest": asset.digest,
            "licence": asset.licence,
            "credit": credit,
        },
        "version": doc.version,
    }))
}

/// `YYYY-MM-DD` in UTC, from the system clock. Used for retrieval dates only; it never
/// enters a hash.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn cmd_log(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let path = project.journal_path();
    let src = fs::read_to_string(&path).unwrap_or_default();
    let entries: Vec<Value> = src
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    Ok(
        json!({ "ok": true, "journal": path.display().to_string(), "entries": entries.len(), "log": entries }),
    )
}

// ---------------------------------------------------------------- args

fn get<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == name {
            return argv.get(i + 1).map(String::as_str);
        }
        if let Some(rest) = a.strip_prefix(&format!("{name}=")) {
            return Some(rest);
        }
        i += 1;
    }
    None
}

fn has(argv: &[String], name: &str) -> bool {
    argv.iter().any(|a| a == name) || argv.iter().any(|a| a.starts_with(&format!("{name}=")))
}

fn positionals(argv: &[String]) -> Vec<String> {
    let value_flags = [
        "--project",
        "--comp",
        "--layer",
        "--frame",
        "-o",
        "--out",
        "--scale",
        "--at",
        "--ops",
        "--expect-version",
    ];
    let mut out = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if value_flags.contains(&a) {
            i += 2;
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        out.push(a.to_string());
        i += 1;
    }
    out
}

fn usage(hint: &str) -> Value {
    json!({
        "ok": false,
        "error": {
            "code": "usage",
            "message": hint,
            "hint": "run `kin capabilities` for the argument list of each verb",
            "fix_op": null,
            "doc": "docs/PLAN.md#contract"
        }
    })
}

fn actor() -> String {
    std::env::var("KIN_ACTOR").unwrap_or_else(|_| "cli".to_string())
}

/// Wall clock for the journal only. It never enters a hash, so renders stay reproducible.
fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_both_forms() {
        let argv: Vec<String> = ["render", "--frame", "3", "--project=./demo"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(get(&argv, "--frame"), Some("3"));
        assert_eq!(get(&argv, "--project"), Some("./demo"));
        assert!(has(&argv, "--project"));
        assert_eq!(positionals(&argv), vec!["render".to_string()]);
    }

    fn call(args: &[&str]) -> Result<Value, Value> {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        dispatch(&argv)
    }

    fn scratch(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("kinema-cli-{name}-{}", std::process::id()));
        p.to_string_lossy().to_string()
    }

    #[test]
    fn fields_projection_keeps_only_what_was_asked_for() {
        let argv: Vec<String> = ["check", "--fields", "ok,errors"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let projected = project_fields(
            json!({ "ok": true, "errors": 0, "warnings": 2, "defects": [] }),
            &argv,
        );
        assert_eq!(projected, json!({ "ok": true, "errors": 0 }));

        // Without the flag nothing is dropped.
        let plain = project_fields(json!({ "ok": true, "n": 1 }), &["check".to_string()]);
        assert_eq!(plain, json!({ "ok": true, "n": 1 }));
    }

    #[test]
    fn diff_reports_the_change_and_its_frame_cost() {
        let root = scratch("diff");
        let _ = fs::remove_dir_all(&root);
        call(&["new", &root]).unwrap();
        let base = format!("{root}/base.json");
        fs::copy(format!("{root}/doc.json"), &base).unwrap();

        // 30 fps, 5 s: 150 frames. The title starts at 0.2 s, so frame 6 is its first.
        let ops = "[{\"op\":\"set-prop\",\"path\":\"comp:main/layer:title/text.color\",\"value\":\"#ffd166\",\"reason\":\"test\"}]";
        let ops_path = format!("{root}/ops.json");
        fs::write(&ops_path, ops).unwrap();
        call(&["apply", "--project", &root, "--ops", &ops_path]).unwrap();

        let report = call(&["diff", "--project", &root, "--from", &base]).unwrap();
        assert_eq!(report["same"], json!(false));
        assert_eq!(report["frames_total"], json!(150));
        assert_eq!(
            report["frames_reusable"],
            json!(6),
            "frames before the title exists"
        );
        assert_eq!(report["frames_invalidated"], json!(144));
        let layers = report["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0]["layer"], json!("title"));
        assert_eq!(layers[0]["fields"], json!(["text.color"]));

        // The same document differs in nothing.
        let same = call(&[
            "diff",
            "--project",
            &root,
            "--from",
            &format!("{root}/doc.json"),
        ])
        .unwrap();
        assert_eq!(same["same"], json!(true));
        assert_eq!(same["frames_invalidated"], json!(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_renders_a_sequence_and_verifies_it() {
        let root = scratch("slice");
        let frames = format!("{root}/frames");
        let _ = fs::remove_dir_all(&root);
        assert_eq!(call(&["new", &root]).unwrap()["ok"], json!(true));

        let out = call(&[
            "render",
            "--project",
            &root,
            "--frames",
            "0-1",
            "-o",
            &frames,
            "--scale",
            "0.25",
        ])
        .unwrap();
        assert_eq!(out["count"], json!(2));
        assert_eq!(out["rendered"], json!(2));

        let manifest = format!("{frames}/manifest.json");
        let verify = call(&["verify", "--project", &root, "--manifest", &manifest]).unwrap();
        assert_eq!(verify["ok"], json!(true), "{verify}");
        assert_eq!(verify["checked"], json!(2));

        // Nothing changed, so the second pass draws nothing.
        let again = call(&[
            "render",
            "--project",
            &root,
            "--frames",
            "0-1",
            "-o",
            &frames,
            "--scale",
            "0.25",
        ])
        .unwrap();
        assert_eq!(again["reused"], json!(2));
        assert_eq!(again["rendered"], json!(0));

        // Re-rendering from the document must reproduce the recorded pixels.
        let fresh = call(&[
            "verify",
            "--project",
            &root,
            "--manifest",
            &manifest,
            "--render",
        ])
        .unwrap();
        assert_eq!(fresh["ok"], json!(true), "{fresh}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn licences_are_looked_up_not_guessed() {
        assert_eq!(licence_flags("cc0-1.0"), Some((false, true, true, false)));
        assert_eq!(licence_flags("CC-BY-4.0"), Some((true, true, true, false)));
        assert_eq!(
            licence_flags("cc-by-sa-4.0"),
            Some((true, true, true, true))
        );
        assert_eq!(
            licence_flags("cc-by-nc-4.0"),
            Some((true, false, true, false))
        );
        assert_eq!(licence_flags("unknown"), Some((false, false, false, false)));
        assert_eq!(
            licence_flags("wtfpl"),
            None,
            "an unrecognised licence must refuse"
        );
    }

    #[test]
    fn today_is_an_iso_date() {
        let d = today();
        assert_eq!(d.len(), 10, "{d}");
        assert_eq!(&d[4..5], "-");
        assert_eq!(&d[7..8], "-");
        let y: i32 = d[..4].parse().unwrap();
        assert!((2024..=9999).contains(&y), "{d}");
    }

    #[test]
    fn kinds_come_from_extensions() {
        assert_eq!(kind_from_path(Path::new("a/b.WAV")), "audio");
        assert_eq!(kind_from_path(Path::new("x.glb")), "model");
        assert_eq!(kind_from_path(Path::new("noextension")), "image");
    }

    #[test]
    fn unknown_verbs_are_typed() {
        let err = not_implemented("audio");
        assert_eq!(err["error"]["code"], "not-implemented");
        assert_eq!(err["error"]["planned"], "P5");
    }

    #[test]
    fn capabilities_lists_the_working_surface() {
        let c = capabilities();
        assert_eq!(c["ok"], true);
        let verbs = c["verbs"].as_array().unwrap();
        assert!(verbs.iter().any(|v| v["name"] == "render"));
        assert_eq!(c["tiers"]["strict"]["available"], true);
        assert_eq!(c["tiers"]["gpu"]["available"], false);
    }
}
