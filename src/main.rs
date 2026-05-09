mod compiler;
mod formatter;
mod ir;
mod lexer;
mod manifest;
mod parser;
mod renderer;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use compiler::{ProjectConfig, SourceLine, compile_events_with_source_map, project_root_for};
use manifest::load_manifest;
use parser::parse_source;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "malison")]
#[command(version)]
#[command(about = "Executable scores for dark electronic music")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse and validate without rendering.
    Check { file: PathBuf },
    /// Validate and print deterministic JSON IR.
    Ir { file: PathBuf },
    /// Validate and print deterministic JSON events.
    Events { file: PathBuf },
    /// Validate and print a deterministic JSON graph.
    Graph {
        file: PathBuf,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Compare two source files by deterministic IR/event output.
    Diff { left: PathBuf, right: PathBuf },
    /// Print backend capability metadata as JSON.
    Capabilities,
    /// Print editor-oriented diagnostics, symbols, hover docs, and completions as JSON.
    LspInfo { file: PathBuf },
    /// Print the deterministic preview-cache path for a source file.
    PreviewCache { file: PathBuf },
    /// Inspect event expansion in a human-readable form.
    Scry { file: PathBuf },
    /// Format a source file in place.
    Fmt {
        file: PathBuf,
        #[arg(long)]
        check: bool,
    },
    /// Compile and render audio.
    Render {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        backend: Option<String>,
        #[arg(long)]
        seed: Option<String>,
        #[arg(long)]
        sample_rate: Option<u32>,
        #[arg(long)]
        bit_depth: Option<u16>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        keep_backend_files: bool,
        #[arg(long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[{}]: {error:#}", diagnostic_code(&error));
            ExitCode::FAILURE
        }
    }
}

fn diagnostic_code(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("unresolved daemon") || message.contains("unresolved spell") {
        "E021"
    } else if message.contains("unsupported")
        || message.contains("expected")
        || message.contains("unexpected")
        || message.contains("unterminated")
        || message.contains("reserved word")
    {
        "E001"
    } else if message.contains("backend") || message.contains("SuperCollider") {
        "E080"
    } else if message.contains("parameter")
        || message.contains("tempo")
        || message.contains("meter")
        || message.contains("must")
        || message.contains("duplicate")
    {
        "E030"
    } else if message.contains("output") || message.contains("failed to read") {
        "E070"
    } else {
        "E000"
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { file } => {
            load_and_compile(&file)?;
            Ok(())
        }
        Command::Ir { file } => {
            let compiled = load_and_compile(&file)?;
            println!("{}", serde_json::to_string_pretty(&compiled.ir)?);
            Ok(())
        }
        Command::Events { file } => {
            let compiled = load_and_compile(&file)?;
            println!("{}", serde_json::to_string_pretty(&compiled.ir)?);
            Ok(())
        }
        Command::Graph { file, format } => {
            let compiled = load_and_compile(&file)?;
            let graph = compiled.ir.graph();
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&graph)?),
                "dot" => println!("{}", graph_dot(&graph)),
                other => anyhow::bail!("graph format `{other}` is not supported"),
            }
            Ok(())
        }
        Command::Diff { left, right } => {
            let left = load_and_compile(&left)?;
            let right = load_and_compile(&right)?;
            print_ir_diff(&left.ir, &right.ir);
            Ok(())
        }
        Command::Capabilities => {
            println!(
                "{}",
                serde_json::to_string_pretty(&renderer::backend_capabilities())?
            );
            Ok(())
        }
        Command::LspInfo { file } => {
            let compiled = load_and_compile(&file)?;
            println!("{}", serde_json::to_string_pretty(&lsp_info(&compiled))?);
            Ok(())
        }
        Command::PreviewCache { file } => {
            let compiled = load_and_compile(&file)?;
            let key = stable_hash(&serde_json::to_string(&compiled.ir)?);
            println!(
                "{}",
                compiled
                    .build_root
                    .join(format!("preview-{key:016x}.wav"))
                    .display()
            );
            Ok(())
        }
        Command::Scry { file } => {
            let compiled = load_and_compile(&file)?;
            print_scry(&compiled);
            Ok(())
        }
        Command::Fmt { file, check } => {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("failed to read `{}`", file.display()))?;
            let formatted = formatter::format_source(&source);
            if check {
                if formatted != source {
                    anyhow::bail!("{} is not formatted", file.display());
                }
                return Ok(());
            }
            if formatted != source {
                fs::write(&file, formatted)
                    .with_context(|| format!("failed to write `{}`", file.display()))?;
            }
            Ok(())
        }
        Command::Render {
            file,
            out,
            backend,
            seed,
            sample_rate,
            bit_depth,
            dry_run,
            keep_backend_files,
            force,
        } => {
            let compiled = load_and_compile(&file)?;
            let backend = backend.unwrap_or_else(|| compiled.render_backend.clone());
            let sample_rate = sample_rate.unwrap_or(compiled.sample_rate);
            let bit_depth = bit_depth.unwrap_or(compiled.bit_depth);

            if !matches!(backend.as_str(), "rust" | "supercollider") {
                anyhow::bail!("backend `{backend}` is not supported in language 0.1");
            }

            let mut compiled = compiled;
            if let Some(seed) = seed {
                compiled.ir.seed = seed;
            }

            let out_path = out.unwrap_or_else(|| compiled.evoke_wav.clone());
            let out_path = if out_path.is_relative() {
                if out_path.components().count() == 1 {
                    compiled.render_root.join(out_path)
                } else {
                    compiled.project_root.join(out_path)
                }
            } else {
                out_path
            };
            if backend == "supercollider" {
                validate_supercollider_feature_support(&compiled)?;
            }
            if dry_run {
                if backend == "supercollider" {
                    let script = renderer::supercollider_script(
                        &compiled,
                        &out_path,
                        sample_rate,
                        bit_depth,
                    )?;
                    if keep_backend_files {
                        write_supercollider_script_artifact(&compiled, &script)?;
                    }
                    println!("{script}");
                } else {
                    println!(
                        "// Generated by Malison 0.1\n// backend: rust\n// working: {}\n// output: {}\n// build: {}\n// events: {}\n",
                        compiled.ir.working,
                        out_path.display(),
                        compiled.build_root.display(),
                        compiled.ir.events.len()
                    );
                }
                return Ok(());
            }

            if out_path.exists() && !force {
                anyhow::bail!(
                    "output `{}` already exists; pass --force to overwrite",
                    out_path.display()
                );
            }
            compiler::validate_output_path(&out_path)?;

            if backend == "supercollider" {
                let script_artifact = if keep_backend_files {
                    Some(supercollider_script_artifact_path(&compiled))
                } else {
                    None
                };
                renderer::render_supercollider(
                    &compiled,
                    &out_path,
                    sample_rate,
                    bit_depth,
                    script_artifact.as_deref(),
                )?;
                write_render_metadata(&compiled, &out_path, &backend, sample_rate, bit_depth)?;
                return Ok(());
            }

            renderer::render_wav(&compiled, &out_path, sample_rate, bit_depth)?;
            write_render_metadata(&compiled, &out_path, &backend, sample_rate, bit_depth)
        }
    }
}

#[derive(Serialize)]
struct RenderMetadata<'a> {
    malison_version: &'a str,
    ir_version: &'a str,
    language: &'a str,
    working: &'a str,
    backend: &'a str,
    sample_rate: u32,
    bit_depth: u16,
    seed: &'a str,
    duration_beats: f64,
    events: usize,
    control_events: usize,
    control_bindings: usize,
}

#[derive(Serialize)]
struct LspInfo {
    diagnostics: Vec<String>,
    symbols: Vec<LspSymbol>,
    hovers: Vec<LspHover>,
    completions: Vec<String>,
}

#[derive(Serialize)]
struct LspSymbol {
    kind: String,
    name: String,
    file: String,
    line: usize,
    column: usize,
}

#[derive(Serialize)]
struct LspHover {
    name: &'static str,
    docs: &'static str,
}

fn write_render_metadata(
    compiled: &compiler::CompiledWorking,
    out_path: &Path,
    backend: &str,
    sample_rate: u32,
    bit_depth: u16,
) -> Result<()> {
    let metadata = RenderMetadata {
        malison_version: env!("CARGO_PKG_VERSION"),
        ir_version: &compiled.ir.ir_version,
        language: &compiled.ir.language,
        working: &compiled.ir.working,
        backend,
        sample_rate,
        bit_depth,
        seed: &compiled.ir.seed,
        duration_beats: compiled.ir.duration_beats,
        events: compiled.ir.events.len(),
        control_events: compiled.ir.control_events.len(),
        control_bindings: compiled.ir.control_bindings.len(),
    };
    let path = out_path.with_extension("malison.json");
    fs::write(&path, serde_json::to_string_pretty(&metadata)?)
        .with_context(|| format!("failed to write `{}`", path.display()))
}

fn lsp_info(compiled: &compiler::CompiledWorking) -> LspInfo {
    let mut symbols = Vec::new();
    for daemon in &compiled.ir.daemons {
        symbols.push(LspSymbol {
            kind: "daemon".to_string(),
            name: daemon.id.clone(),
            file: daemon.source.file.clone(),
            line: daemon.source.line,
            column: daemon.source.column,
        });
    }
    for spell in &compiled.ir.spells {
        symbols.push(LspSymbol {
            kind: "spell".to_string(),
            name: spell.id.clone(),
            file: spell.source.file.clone(),
            line: spell.source.line,
            column: spell.source.column,
        });
    }
    for rite in &compiled.ir.rites {
        symbols.push(LspSymbol {
            kind: "rite".to_string(),
            name: rite.id.clone(),
            file: rite.source.file.clone(),
            line: rite.source.line,
            column: rite.source.column,
        });
    }
    for circle in &compiled.ir.circles {
        symbols.push(LspSymbol {
            kind: "circle".to_string(),
            name: circle.id.clone(),
            file: circle.source.file.clone(),
            line: circle.source.line,
            column: circle.source.column,
        });
    }
    LspInfo {
        diagnostics: Vec::new(),
        symbols,
        hovers: vec![
            LspHover {
                name: "gain",
                docs: "Gain in decibels.",
            },
            LspHover {
                name: "pan",
                docs: "Stereo pan from -1 left to 1 right.",
            },
            LspHover {
                name: "cutoff",
                docs: "Filter cutoff in hertz.",
            },
            LspHover {
                name: "drive",
                docs: "Saturation drive normalized in [0, 1].",
            },
        ],
        completions: vec![
            "circle".to_string(),
            "daemon".to_string(),
            "spell".to_string(),
            "rite".to_string(),
            "invoke".to_string(),
            "bind".to_string(),
            "raise".to_string(),
            "lower".to_string(),
            "banish".to_string(),
            "sample".to_string(),
            "samplekit".to_string(),
            "saw_sub".to_string(),
            "drone".to_string(),
            "noise_burst".to_string(),
            "swarm".to_string(),
            "metal_hit".to_string(),
        ],
    }
}

fn validate_supercollider_feature_support(compiled: &compiler::CompiledWorking) -> Result<()> {
    if compiled
        .ir
        .circles
        .iter()
        .any(|circle| !circle.effects.is_empty() || !circle.wards.is_empty())
    {
        anyhow::bail!("backend `supercollider` does not support circle effects or wards yet");
    }
    if compiled.ir.daemons.iter().any(|daemon| {
        daemon
            .params
            .get("out")
            .and_then(|value| value.as_str())
            .is_some_and(|out| out != "master")
    }) {
        anyhow::bail!("backend `supercollider` does not support audio bus routing yet");
    }
    Ok(())
}

fn supercollider_script_artifact_path(compiled: &compiler::CompiledWorking) -> PathBuf {
    compiled.build_root.join("malison-supercollider.scd")
}

fn write_supercollider_script_artifact(
    compiled: &compiler::CompiledWorking,
    script: &str,
) -> Result<()> {
    let path = supercollider_script_artifact_path(compiled);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }
    fs::write(&path, script).with_context(|| format!("failed to write `{}`", path.display()))
}

fn graph_dot(graph: &ir::IrGraph) -> String {
    let mut dot = String::from("digraph malison {\n");
    for node in &graph.nodes {
        dot.push_str(&format!(
            "  \"{}\" [label=\"{}\", shape={}];\n",
            dot_escape(&node.id),
            dot_escape(&node.label),
            match node.kind.as_str() {
                "event" => "point",
                "circle" => "box",
                "control" => "diamond",
                _ => "ellipse",
            }
        ));
    }
    for edge in &graph.edges {
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            dot_escape(&edge.from),
            dot_escape(&edge.to),
            dot_escape(&edge.kind)
        ));
    }
    dot.push_str("}\n");
    dot
}

fn dot_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn print_ir_diff(left: &ir::Ir, right: &ir::Ir) {
    println!("working: {} -> {}", left.working, right.working);
    println!(
        "events: {} -> {} ({:+})",
        left.events.len(),
        right.events.len(),
        right.events.len() as isize - left.events.len() as isize
    );
    println!(
        "control_events: {} -> {} ({:+})",
        left.control_events.len(),
        right.control_events.len(),
        right.control_events.len() as isize - left.control_events.len() as isize
    );
    println!(
        "control_bindings: {} -> {} ({:+})",
        left.control_bindings.len(),
        right.control_bindings.len(),
        right.control_bindings.len() as isize - left.control_bindings.len() as isize
    );

    let left_ids = left
        .events
        .iter()
        .map(|event| event.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let right_ids = right
        .events
        .iter()
        .map(|event| event.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let added = right_ids.difference(&left_ids).count();
    let removed = left_ids.difference(&right_ids).count();
    println!("event_ids_added: {added}");
    println!("event_ids_removed: {removed}");
}

fn load_and_compile(path: &Path) -> Result<compiler::CompiledWorking> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("rite") {
        anyhow::bail!(
            "source file `{}` must use the .rite extension",
            path.display()
        );
    }
    let raw_source =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let expanded = expand_includes(path, &raw_source, &mut HashSet::new())?;
    let working = parse_source(path, &expanded.source)
        .map_err(|error| with_source_snippet(path, &expanded.source, error))?;
    let project_root = project_root_for(path)?;
    let manifest = load_manifest(&project_root)?;
    let config = ProjectConfig {
        sample_dir: manifest.paths.samples.clone(),
        sample_libraries: manifest.paths.sample_libraries.clone(),
        render_dir: manifest.paths.renders.clone(),
        build_dir: manifest.paths.build.clone(),
    };
    let mut compiled =
        compile_events_with_source_map(path, Some(&expanded.map), &project_root, &config, working)
            .map_err(|error| with_source_snippet(path, &expanded.source, error))?;
    compiled.render_backend = manifest.render.backend;
    compiled.sample_rate = manifest.render.sample_rate;
    compiled.bit_depth = manifest.render.bit_depth;
    Ok(compiled)
}

struct ExpandedSource {
    source: String,
    map: Vec<SourceLine>,
}

fn expand_includes(
    path: &Path,
    source: &str,
    seen: &mut HashSet<PathBuf>,
) -> Result<ExpandedSource> {
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize `{}`", path.display()))?;
    if !seen.insert(canonical.clone()) {
        anyhow::bail!("include cycle involving `{}`", path.display());
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut expanded = String::new();
    let mut map = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        if let Some(include) = parse_include_line(line) {
            let include_path = base.join(include);
            if include_path.extension().and_then(|ext| ext.to_str()) != Some("rite") {
                anyhow::bail!(
                    "include `{}` must use the .rite extension",
                    include_path.display()
                );
            }
            let include_source = fs::read_to_string(&include_path)
                .with_context(|| format!("failed to read `{}`", include_path.display()))?;
            let include = expand_includes(&include_path, &include_source, seen)?;
            expanded.push_str(&include.source);
            map.extend(include.map);
            expanded.push('\n');
            map.push(SourceLine {
                file: include_path.clone(),
                line: include_source.lines().count().saturating_add(1),
            });
        } else {
            expanded.push_str(line);
            expanded.push('\n');
            map.push(SourceLine {
                file: path.to_path_buf(),
                line: line_index + 1,
            });
        }
    }
    seen.remove(&canonical);
    Ok(ExpandedSource {
        source: expanded,
        map,
    })
}

fn parse_include_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("include ")?;
    rest.strip_prefix('"')?.strip_suffix('"')
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn with_source_snippet(path: &Path, source: &str, error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    let Some((line, column)) = parse_leading_span(&message) else {
        return error;
    };
    let Some(source_line) = source.lines().nth(line.saturating_sub(1)) else {
        return error;
    };
    let line_number = line.to_string();
    let gutter = " ".repeat(line_number.len());
    anyhow::anyhow!(
        "{message}\n--> {}:{line}:{column}\n{line_number} | {source_line}\n{gutter} | {}^",
        path.display(),
        " ".repeat(column.saturating_sub(1))
    )
}

fn parse_leading_span(message: &str) -> Option<(usize, usize)> {
    let (line, rest) = message.split_once(':')?;
    let (column, _) = rest.split_once(':')?;
    Some((line.parse().ok()?, column.parse().ok()?))
}

fn print_scry(compiled: &compiler::CompiledWorking) {
    let ir = &compiled.ir;
    println!("working: {}", ir.working);
    println!("language: {}", ir.language);
    println!("tempo: {} bpm", ir.tempo_bpm);
    println!("meter: {}/{}", ir.meter[0], ir.meter[1]);
    println!("duration: {} beats", ir.duration_beats);
    println!("daemons: {}", ir.daemons.len());
    println!("spells: {}", ir.spells.len());
    println!("rites: {}", ir.rites.len());
    println!("control events: {}", ir.control_events.len());
    println!("control bindings: {}", ir.control_bindings.len());
    println!("events: {}", ir.events.len());
    for rite in &ir.rites {
        println!(
            "\nrite {}: start {} beats, duration {} beats",
            rite.id, rite.start_beats, rite.duration_beats
        );
        for event in ir.events.iter().filter(|event| {
            event.time_beats >= rite.start_beats
                && event.time_beats < rite.start_beats + rite.duration_beats
        }) {
            if let Some(pitch) = &event.pitch {
                println!(
                    "  {:>7.3} {:<7} {:<12} velocity {:>4.2} pitch {} ({})",
                    event.time_beats,
                    event.kind,
                    event.daemon,
                    event.velocity,
                    pitch.name,
                    pitch.midi
                );
            } else {
                println!(
                    "  {:>7.3} {:<7} {:<12} velocity {:>4.2}",
                    event.time_beats, event.kind, event.daemon, event.velocity
                );
            }
        }
        for control in ir.control_events.iter().filter(|control| {
            control.start_beats >= rite.start_beats
                && control.start_beats < rite.start_beats + rite.duration_beats
        }) {
            println!(
                "  {:>7.3} control {:<12} {:>4.2} -> {:>4.2}",
                control.start_beats, control.target, control.from, control.to
            );
        }
        for binding in ir.control_bindings.iter().filter(|binding| {
            binding.start_beats >= rite.start_beats
                && binding.start_beats < rite.start_beats + rite.duration_beats
        }) {
            println!(
                "  {:>7.3} bind {:<12} to {:<12} {:>6.2} -> {:>6.2}",
                binding.start_beats,
                format!("{}.{}", binding.target_daemon, binding.target_param),
                binding.source,
                binding.from,
                binding.to
            );
        }
    }
}
