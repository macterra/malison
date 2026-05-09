mod compiler;
mod formatter;
mod ir;
mod lexer;
mod manifest;
mod parser;
mod renderer;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use compiler::{ProjectConfig, compile_events, project_root_for};
use manifest::load_manifest;
use parser::parse_source;

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
            if dry_run {
                if backend == "supercollider" {
                    let script = renderer::supercollider_script(
                        &compiled,
                        &out_path,
                        sample_rate,
                        bit_depth,
                    )?;
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
                return renderer::render_supercollider(
                    &compiled,
                    &out_path,
                    sample_rate,
                    bit_depth,
                );
            }

            renderer::render_wav(&compiled, &out_path, sample_rate, bit_depth)
        }
    }
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
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let working =
        parse_source(path, &source).map_err(|error| with_source_snippet(path, &source, error))?;
    let project_root = project_root_for(path)?;
    let manifest = load_manifest(&project_root)?;
    let config = ProjectConfig {
        sample_dir: manifest.paths.samples.clone(),
        render_dir: manifest.paths.renders.clone(),
        build_dir: manifest.paths.build.clone(),
    };
    let mut compiled = compile_events(path, &project_root, &config, working)
        .map_err(|error| with_source_snippet(path, &source, error))
        ?;
    compiled.render_backend = manifest.render.backend;
    compiled.sample_rate = manifest.render.sample_rate;
    compiled.bit_depth = manifest.render.bit_depth;
    Ok(compiled)
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
    }
}
