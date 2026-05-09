mod compiler;
mod lexer;
mod parser;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use compiler::{compile_events, project_root_for};
use parser::parse_source;

#[derive(Debug, Parser)]
#[command(name = "malison")]
#[command(about = "Executable scores for dark electronic music")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse and validate without rendering.
    Check { file: PathBuf },
    /// Validate and print deterministic JSON events.
    Events { file: PathBuf },
    /// Compile and render audio.
    Render {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "supercollider")]
        backend: String,
        #[arg(long)]
        seed: Option<String>,
        #[arg(long, default_value_t = 48000)]
        sample_rate: u32,
        #[arg(long, default_value_t = 24)]
        bit_depth: u16,
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
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { file } => {
            load_and_compile(&file)?;
            Ok(())
        }
        Command::Events { file } => {
            let compiled = load_and_compile(&file)?;
            println!("{}", serde_json::to_string_pretty(&compiled.ir)?);
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
            if backend != "supercollider" {
                anyhow::bail!("backend `{backend}` is not supported in language 0.1");
            }

            let mut compiled = load_and_compile(&file)?;
            if let Some(seed) = seed {
                compiled.ir.seed = seed;
            }

            let out_path = out.unwrap_or_else(|| compiled.evoke_wav.clone());
            if out_path.exists() && !force {
                anyhow::bail!(
                    "output `{}` already exists; pass --force to overwrite",
                    out_path.display()
                );
            }

            let script = compiler::supercollider_script(&compiled, sample_rate, bit_depth);
            if dry_run {
                println!("{script}");
                return Ok(());
            }

            anyhow::bail!(
                "render execution is not implemented yet; use --dry-run to inspect generated SuperCollider source"
            )
        }
    }
}

fn load_and_compile(path: &Path) -> Result<compiler::CompiledWorking> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("rite") {
        anyhow::bail!("source files must use the .rite extension");
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let working = parse_source(path, &source)?;
    let project_root = project_root_for(path)?;
    compile_events(path, &project_root, working)
}
