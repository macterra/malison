use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::parser::{DaemonKind, PatternKind, Value, Working};

#[derive(Clone, Debug)]
pub struct CompiledWorking {
    pub ir: Ir,
    pub evoke_wav: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct Ir {
    pub language: String,
    pub working: String,
    pub tempo_bpm: f64,
    pub meter: [u32; 2],
    pub seed: String,
    pub duration_beats: f64,
    pub daemons: Vec<IrDaemon>,
    pub spells: Vec<IrSpell>,
    pub rites: Vec<IrRite>,
    pub events: Vec<IrEvent>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrDaemon {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
    pub params: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrSpell {
    pub id: String,
    pub kind: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrRite {
    pub id: String,
    pub start_beats: f64,
    pub duration_beats: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrEvent {
    pub id: String,
    pub kind: String,
    pub time_beats: f64,
    pub duration_beats: f64,
    pub daemon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<IrPitch>,
    pub params: BTreeMap<String, serde_json::Value>,
    pub source: IrSource,
    #[serde(skip_serializing)]
    pub source_order: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrPitch {
    pub name: String,
    pub midi: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrSource {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

pub fn project_root_for(input: &Path) -> Result<PathBuf> {
    let start = input
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut cursor = fs::canonicalize(start)
        .with_context(|| format!("failed to canonicalize `{}`", start.display()))?;
    loop {
        if cursor.join("malison.toml").exists() {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return std::env::current_dir().context("failed to read current directory");
        }
    }
}

pub fn compile_events(
    input: &Path,
    project_root: &Path,
    working: Working,
) -> Result<CompiledWorking> {
    validate_unique_names(
        "daemon",
        working.daemons.iter().map(|daemon| daemon.name.as_str()),
    )?;
    validate_unique_names(
        "spell",
        working.spells.iter().map(|spell| spell.name.as_str()),
    )?;
    validate_unique_names("rite", working.rites.iter().map(|rite| rite.name.as_str()))?;

    let daemon_map = working
        .daemons
        .iter()
        .map(|daemon| (daemon.name.as_str(), daemon))
        .collect::<BTreeMap<_, _>>();
    let spell_map = working
        .spells
        .iter()
        .map(|spell| (spell.name.as_str(), spell))
        .collect::<BTreeMap<_, _>>();

    for daemon in &working.daemons {
        if daemon.kind == DaemonKind::Sample {
            let sample = daemon.sample_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!("sample daemon `{}` is missing a path", daemon.name)
            })?;
            validate_sample_path(project_root, sample)?;
        }
        validate_params(&daemon.name, daemon.kind.clone(), &daemon.params)?;
    }

    let mut events = Vec::new();
    let mut rites = Vec::new();
    let mut cursor_beats = 0.0;
    for rite in &working.rites {
        let duration_beats = rite.bars as f64 * working.meter.0 as f64;
        rites.push(IrRite {
            id: rite.name.clone(),
            start_beats: cursor_beats,
            duration_beats,
        });

        for invoke in &rite.invokes {
            let daemon = daemon_map.get(invoke.daemon.as_str()).ok_or_else(|| {
                anyhow::anyhow!("{}: unresolved daemon `{}`", invoke.span, invoke.daemon)
            })?;
            validate_params(&invoke.daemon, daemon.kind.clone(), &invoke.params)?;
            let params = merged_params(&daemon.params, &invoke.params);

            match &invoke.spell {
                Some(spell_name) => {
                    let spell = spell_map.get(spell_name.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("{}: unresolved spell `{spell_name}`", invoke.span)
                    })?;
                    match (&daemon.kind, &spell.kind) {
                        (DaemonKind::Sample, PatternKind::Rhythm) => {
                            expand_rhythm(
                                input,
                                &mut events,
                                &rite.name,
                                cursor_beats,
                                duration_beats,
                                invoke,
                                &spell.body,
                                &params,
                            )?;
                        }
                        (DaemonKind::SawSub, PatternKind::Notes) => {
                            expand_notes(
                                input,
                                &mut events,
                                &rite.name,
                                cursor_beats,
                                duration_beats,
                                invoke,
                                &spell.body,
                                &params,
                            )?;
                        }
                        _ => bail!(
                            "{}: daemon `{}` cannot be invoked with spell `{spell_name}`",
                            invoke.span,
                            invoke.daemon
                        ),
                    }
                }
                None => {
                    events.push(IrEvent {
                        id: format!(
                            "evt_{}_{}_once",
                            sanitize_id(&rite.name),
                            invoke.source_order
                        ),
                        kind: match daemon.kind {
                            DaemonKind::Sample => "trigger".to_string(),
                            DaemonKind::SawSub => "note".to_string(),
                        },
                        time_beats: cursor_beats,
                        duration_beats: invoke.every.map(|duration| duration.beats).unwrap_or(0.25),
                        daemon: invoke.daemon.clone(),
                        pitch: None,
                        params,
                        source: source_for(input, invoke),
                        source_order: invoke.source_order,
                    });
                }
            }
        }
        cursor_beats += duration_beats;
    }

    events.sort_by(|a, b| {
        a.time_beats
            .total_cmp(&b.time_beats)
            .then(a.source_order.cmp(&b.source_order))
            .then(a.kind.cmp(&b.kind))
            .then(a.id.cmp(&b.id))
    });

    let ir = Ir {
        language: "0.1".to_string(),
        working: working.name,
        tempo_bpm: working.tempo_bpm,
        meter: [working.meter.0, working.meter.1],
        seed: working.seed,
        duration_beats: cursor_beats,
        daemons: working
            .daemons
            .iter()
            .map(|daemon| IrDaemon {
                id: daemon.name.clone(),
                kind: match daemon.kind {
                    DaemonKind::Sample => "sample".to_string(),
                    DaemonKind::SawSub => "saw_sub".to_string(),
                },
                sample: daemon.sample_path.clone(),
                params: merged_params(&daemon.params, &[]),
            })
            .collect(),
        spells: working
            .spells
            .iter()
            .map(|spell| IrSpell {
                id: spell.name.clone(),
                kind: match spell.kind {
                    PatternKind::Rhythm => "pattern".to_string(),
                    PatternKind::Notes => "notes".to_string(),
                },
                body: spell.body.clone(),
            })
            .collect(),
        rites,
        events,
    };

    Ok(CompiledWorking {
        evoke_wav: PathBuf::from(working.evoke_wav),
        ir,
    })
}

pub fn supercollider_script(
    compiled: &CompiledWorking,
    sample_rate: u32,
    bit_depth: u16,
) -> String {
    format!(
        "// Generated by Malison 0.1\n// working: {}\n// sample_rate: {}\n// bit_depth: {}\n// events: {}\n",
        compiled.ir.working,
        sample_rate,
        bit_depth,
        compiled.ir.events.len()
    )
}

fn expand_rhythm(
    input: &Path,
    events: &mut Vec<IrEvent>,
    rite_name: &str,
    rite_start: f64,
    rite_duration: f64,
    invoke: &crate::parser::Invoke,
    body: &str,
    params: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let steps = rhythm_steps(body)?;
    let step_duration = invoke.every.map(|duration| duration.beats).unwrap_or(0.25);
    let total_steps = (rite_duration / step_duration).ceil() as usize;
    for absolute_step in 0..total_steps {
        let time = absolute_step as f64 * step_duration;
        if time >= rite_duration {
            break;
        }
        if steps[absolute_step % steps.len()] {
            events.push(IrEvent {
                id: format!(
                    "evt_{}_{}_{}",
                    sanitize_id(rite_name),
                    invoke.source_order,
                    absolute_step
                ),
                kind: "trigger".to_string(),
                time_beats: rite_start + time,
                duration_beats: step_duration,
                daemon: invoke.daemon.clone(),
                pitch: None,
                params: params.clone(),
                source: source_for(input, invoke),
                source_order: invoke.source_order,
            });
        }
    }
    Ok(())
}

fn expand_notes(
    input: &Path,
    events: &mut Vec<IrEvent>,
    rite_name: &str,
    rite_start: f64,
    rite_duration: f64,
    invoke: &crate::parser::Invoke,
    body: &str,
    params: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let steps = note_steps(body)?;
    let step_duration = invoke.every.map(|duration| duration.beats).unwrap_or(0.5);
    let total_steps = (rite_duration / step_duration).ceil() as usize;
    for absolute_step in 0..total_steps {
        let time = absolute_step as f64 * step_duration;
        if time >= rite_duration {
            break;
        }
        if let Some(pitch_name) = &steps[absolute_step % steps.len()] {
            events.push(IrEvent {
                id: format!(
                    "evt_{}_{}_{}",
                    sanitize_id(rite_name),
                    invoke.source_order,
                    absolute_step
                ),
                kind: "note".to_string(),
                time_beats: rite_start + time,
                duration_beats: step_duration,
                daemon: invoke.daemon.clone(),
                pitch: Some(IrPitch {
                    name: pitch_name.clone(),
                    midi: pitch_to_midi(pitch_name)?,
                }),
                params: params.clone(),
                source: source_for(input, invoke),
                source_order: invoke.source_order,
            });
        }
    }
    Ok(())
}

fn rhythm_steps(body: &str) -> Result<Vec<bool>> {
    let mut steps = Vec::new();
    for ch in body.chars() {
        match ch {
            'x' => steps.push(true),
            '-' => steps.push(false),
            ' ' => {}
            other => bail!("unsupported rhythm pattern character `{other}`"),
        }
    }
    if steps.is_empty() {
        bail!("rhythm pattern cannot be empty");
    }
    Ok(steps)
}

fn note_steps(body: &str) -> Result<Vec<Option<String>>> {
    let mut steps = Vec::new();
    for part in body.split_whitespace() {
        match part {
            "-" => steps.push(None),
            "|" => {}
            pitch if looks_like_pitch(pitch) => steps.push(Some(pitch.to_string())),
            other => bail!("unsupported note pattern token `{other}`"),
        }
    }
    if steps.is_empty() {
        bail!("note pattern cannot be empty");
    }
    Ok(steps)
}

fn looks_like_pitch(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='G')) && chars.any(|ch| ch.is_ascii_digit())
}

fn pitch_to_midi(value: &str) -> Result<i32> {
    let chars = value.chars().collect::<Vec<_>>();
    let root = chars[0];
    let mut index = 1;
    let accidental = if matches!(chars.get(index), Some('b' | '#')) {
        let accidental = chars[index];
        index += 1;
        accidental
    } else {
        ' '
    };
    let octave = value[index..].parse::<i32>()?;
    let semitone = match root {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => unreachable!(),
    } + match accidental {
        '#' => 1,
        'b' => -1,
        _ => 0,
    };
    Ok((octave + 1) * 12 + semitone)
}

fn validate_unique_names<'a>(kind: &str, names: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.to_string()) {
            bail!("duplicate {kind} `{name}`");
        }
    }
    Ok(())
}

fn validate_sample_path(project_root: &Path, sample: &str) -> Result<()> {
    if sample.starts_with('~') || sample.contains('*') || sample.contains("://") {
        bail!("sample path `{sample}` is not valid in language 0.1");
    }
    let path = project_root.join(sample);
    if !path.exists() {
        bail!("sample file `{}` does not exist", path.display());
    }
    Ok(())
}

fn validate_params(owner: &str, kind: DaemonKind, params: &[crate::parser::Param]) -> Result<()> {
    for param in params {
        let allowed = match kind {
            DaemonKind::Sample => matches!(
                param.name.as_str(),
                "gain" | "pan" | "tune" | "highpass" | "lowpass"
            ),
            DaemonKind::SawSub => {
                matches!(param.name.as_str(), "gain" | "pan" | "cutoff" | "drive")
            }
        };
        if !allowed {
            bail!("`{}` does not support parameter `{}`", owner, param.name);
        }
    }
    Ok(())
}

fn merged_params(
    daemon_params: &[crate::parser::Param],
    invoke_params: &[crate::parser::Param],
) -> BTreeMap<String, serde_json::Value> {
    let mut params = BTreeMap::new();
    for param in daemon_params.iter().chain(invoke_params) {
        params.insert(
            canonical_param_name(&param.name),
            value_to_json(&param.value),
        );
    }
    params
}

fn canonical_param_name(name: &str) -> String {
    match name {
        "gain" => "gain_db",
        "cutoff" => "cutoff_hz",
        "highpass" => "highpass_hz",
        "lowpass" => "lowpass_hz",
        "tune" => "tune_semitones",
        other => other,
    }
    .to_string()
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Number(number) => serde_json::json!(number),
        Value::String(string) => serde_json::json!(string),
        Value::Pitch(pitch) => serde_json::json!(pitch),
    }
}

fn source_for(input: &Path, invoke: &crate::parser::Invoke) -> IrSource {
    IrSource {
        file: input.display().to_string(),
        line: invoke.span.line,
        column: invoke.span.column,
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::parser::{PatternKind, parse_source};

    #[test]
    fn expands_patterns_to_rite_boundary() {
        let root = std::env::temp_dir().join(format!("malison-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  daemon bass = saw_sub

  spell kicks = pattern "x---"
  spell bassline = notes "F1 -"

  rite main bars 1 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        assert_eq!(working.spells[0].kind, PatternKind::Rhythm);

        let compiled = compile_events(&path, &root, working).unwrap();
        let triggers = compiled
            .ir
            .events
            .iter()
            .filter(|event| event.kind == "trigger")
            .count();
        let notes = compiled
            .ir
            .events
            .iter()
            .filter(|event| event.kind == "note")
            .count();
        assert_eq!(triggers, 4);
        assert_eq!(notes, 4);

        fs::remove_dir_all(&root).unwrap();
    }
}
