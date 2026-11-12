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
            print_json(&v, pretty);
        }
        Err(e) => {
            print_json(&e, pretty);
            std::process::exit(1);
        }
    }
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
        "schema" => Ok(schema::doc_schema()),
        "help" | "--help" | "-h" => Ok(help()),
        "version" | "--version" | "-V" => {
            Ok(json!({ "ok": true, "kin": VERSION, "schema": kinema_doc::model::SCHEMA_TAG }))
        }
        "new" => cmd_new(argv),
        "probe" => cmd_probe(argv),
        "apply" => cmd_apply(argv),
        "render" => cmd_render(argv),
        "check" => cmd_check(argv),
        "credits" => cmd_credits(argv),
        "log" => cmd_log(argv),
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
        "font" | "typeface" | "type" => (
            "the font library and modular scale land in P4; this build draws a bitmap face",
            "P4",
        ),
        "palette" => (
            "OKLab palette extraction lands in P4; PALETTE already scores the result",
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
            "verbs": ["capabilities", "schema", "new", "probe", "apply", "render", "check", "credits", "log"],
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
            {"name": "apply", "summary": "apply typed ops, returning their inverses", "args": ["--ops <file|->", "--expect-version <n>", "--dry-run"]},
            {"name": "render", "summary": "render one frame to PNG", "args": ["--frame <n>", "-o <file.png>", "--scale <f>", "--comp <id>"]},
            {"name": "check", "summary": "run craft detectors", "args": ["--comp <id>", "--frame <n>"]},
            {"name": "credits", "summary": "licence roll-up and release gate", "args": []},
            {"name": "log", "summary": "the op journal", "args": []}
        ],
        "ops": kinema_doc::ops::op_schema(),
        "checks": [
            {"code": "TYPE-FAKE", "severity": "error", "detects": "synthesised italic/bold/small caps, unbundled family"},
            {"code": "READ", "severity": "error", "detects": "text under its contrast target against the ground it sits on"},
            {"code": "PALETTE", "severity": "warn", "detects": "more than one accent hue"},
            {"code": "STAGGER", "severity": "warn", "detects": "more than 70% of layers entering on one frame"}
        ],
        "checks_pending": ["EASE-FLAT", "HOLD-NONE", "SHUTTER", "CUT-TIMING", "JUMPCUT", "CHROMA", "BANDING", "LOUD"],
        "layer_kinds": ["solid", "text", "shape"],
        "layer_kinds_pending": ["image", "comp", "adjustment"],
        "props": ["value", "kf", "expr", "link", "graph"],
        "props_working": ["value", "kf", "link"],
        "fonts": checks::BUNDLED_FONTS,
        "weights": checks::BUNDLED_WEIGHTS,
        "path_syntax": "comp:main/layer:title/transform.position",
        "error_contract": {"code": "string", "message": "string", "hint": "string", "fix_op": "op|null", "doc": "string"},
        "stateless": "every verb reads the project and writes it back; `--expect-version` gives compare-and-swap for parallel agents",
    })
}

fn help() -> Value {
    json!({
        "ok": true,
        "usage": "kin <verb> [args] [--project <dir>] [--pretty]",
        "verbs": ["capabilities", "schema", "new", "probe", "apply", "render", "check", "credits", "log"],
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
            "kin check --project <dir>"
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
        "skipped": rendered.skipped,
        "ms": ms,
    }))
}

fn cmd_check(argv: &[String]) -> R {
    let project = Project::open(argv)?;
    let doc = project.load()?;
    let comp = get(argv, "--comp").unwrap_or("main").to_string();
    let frame = get(argv, "--frame").and_then(|s| s.parse::<u32>().ok());
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
