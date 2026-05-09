use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::ir::{
    Ir, IrCircle, IrControlEvent, IrDaemon, IrEvent, IrPitch, IrRandomStream, IrRenderTarget,
    IrRite, IrSource, IrSpell, IrWard,
};
use crate::parser::{
    AutomationCurve, AutomationDirection, DaemonKind, PatternKind, PatternTransform, RitePlacement,
    Value, Working,
};

#[derive(Clone, Debug)]
pub struct CompiledWorking {
    pub ir: Ir,
    pub evoke_wav: PathBuf,
    pub project_root: PathBuf,
    pub sample_root: PathBuf,
    pub render_root: PathBuf,
    pub build_root: PathBuf,
    pub render_backend: String,
    pub sample_rate: u32,
    pub bit_depth: u16,
}

#[derive(Clone, Debug)]
pub struct ProjectConfig {
    pub sample_dir: PathBuf,
    pub render_dir: PathBuf,
    pub build_dir: PathBuf,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            sample_dir: PathBuf::from("samples"),
            render_dir: PathBuf::from("renders"),
            build_dir: PathBuf::from("build"),
        }
    }
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
    config: &ProjectConfig,
    working: Working,
) -> Result<CompiledWorking> {
    validate_working_header(&working)?;
    validate_unique_names(
        "circle",
        working.circles.iter().map(|circle| circle.name.as_str()),
    )?;
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
    validate_circles(&working)?;
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
            validate_sample_path(project_root, config, sample)?;
        }
        if daemon.kind == DaemonKind::SampleKit {
            let sample_dir = daemon.sample_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!("samplekit daemon `{}` is missing a path", daemon.name)
            })?;
            validate_samplekit_path(project_root, config, sample_dir)?;
        }
        validate_params(&daemon.name, daemon.kind.clone(), &daemon.params)?;
    }

    validate_invokes(&working, &daemon_map, &spell_map)?;

    let mut events = Vec::new();
    let mut control_events = Vec::new();
    let mut rites = Vec::new();
    let mut cursor_beats = 0.0;
    let mut occupied_ranges = Vec::<(String, f64, f64)>::new();
    for rite in &working.rites {
        if rite.bars == 0 {
            bail!("rite `{}` must have a positive bar count", rite.name);
        }
        if rite.invokes.is_empty() {
            bail!("rite `{}` must contain at least one invoke", rite.name);
        }
        let duration_beats = rite.bars as f64 * working.meter.0 as f64;
        let rite_start = match rite.placement {
            Some(RitePlacement::Bar(bar)) => {
                if bar == 0 {
                    bail!("rite `{}` cannot start at bar 0", rite.name);
                }
                (bar - 1) as f64 * working.meter.0 as f64
            }
            Some(RitePlacement::Seconds(seconds)) => seconds * working.tempo_bpm / 60.0,
            None => cursor_beats,
        };
        if !rite.layer {
            for (other, start, end) in &occupied_ranges {
                if ranges_overlap(rite_start, rite_start + duration_beats, *start, *end) {
                    bail!(
                        "{}: rite `{}` overlaps rite `{other}`; add `layer` to allow overlap",
                        rite.span,
                        rite.name
                    );
                }
            }
            occupied_ranges.push((rite.name.clone(), rite_start, rite_start + duration_beats));
        }
        rites.push(IrRite {
            id: rite.name.clone(),
            start_beats: rite_start,
            duration_beats,
            layer: rite.layer,
            source: source_for_span(input, rite.span),
        });

        for (automation_index, automation) in rite.automations.iter().enumerate() {
            validate_control_target(&automation.target, automation.span)?;
            let (id, semantic_path) = control_identity(&rite.name, automation_index);
            let (from, to) = match automation.direction {
                AutomationDirection::Raise => (automation.from, automation.to),
                AutomationDirection::Lower => (automation.from, automation.to),
            };
            validate_automation_curve(&automation.curve, from, to, automation.span)?;
            control_events.push(IrControlEvent {
                id,
                semantic_path,
                target: automation.target.clone(),
                curve: automation_curve_label(&automation.curve).to_string(),
                start_beats: rite_start,
                duration_beats,
                from,
                to,
                source: source_for_span(input, automation.span),
            });
        }

        for banish in &rite.banishes {
            if !daemon_map.contains_key(banish.daemon.as_str()) {
                bail!(
                    "{}",
                    unresolved_name(
                        "daemon",
                        &banish.daemon,
                        daemon_map.keys().copied(),
                        banish.span,
                    )
                );
            }
            truncate_continuous_events(&mut events, &banish.daemon, rite_start);
            let (id, semantic_path) = event_identity(&rite.name, banish.source_order, "banish");
            events.push(IrEvent {
                id,
                semantic_path,
                kind: "banish".to_string(),
                time_beats: rite_start,
                duration_beats: 0.0,
                daemon: banish.daemon.clone(),
                velocity: 0.0,
                pitch: None,
                params: BTreeMap::new(),
                source: source_for_span(input, banish.span),
                source_order: banish.source_order,
            });
        }

        for invoke in &rite.invokes {
            if let Some(every) = invoke.every
                && every.beats <= 0.0
            {
                bail!("{}: `every` duration must be positive", invoke.span);
            }
            let daemon = daemon_map.get(invoke.daemon.as_str()).ok_or_else(|| {
                unresolved_name(
                    "daemon",
                    &invoke.daemon,
                    daemon_map.keys().copied(),
                    invoke.span,
                )
            })?;
            validate_params(&invoke.daemon, daemon.kind.clone(), &invoke.params)?;
            let params = merged_params(&daemon.params, &invoke.params);

            match &invoke.spell {
                Some(spell_name) => {
                    let spell = spell_map.get(spell_name.as_str()).ok_or_else(|| {
                        unresolved_name("spell", spell_name, spell_map.keys().copied(), invoke.span)
                    })?;
                    match (&daemon.kind, &spell.kind) {
                        (
                            DaemonKind::Sample
                            | DaemonKind::SampleKit
                            | DaemonKind::NoiseBurst
                            | DaemonKind::MetalHit,
                            PatternKind::Rhythm,
                        ) => {
                            let context = ExpansionContext::new(
                                input,
                                &rite.name,
                                &working.seed,
                                rite_start,
                                duration_beats,
                            );
                            expand_rhythm(&context, &mut events, invoke, spell, &params)?;
                        }
                        (DaemonKind::SawSub | DaemonKind::Swarm, PatternKind::Notes) => {
                            let context = ExpansionContext::new(
                                input,
                                &rite.name,
                                &working.seed,
                                rite_start,
                                duration_beats,
                            );
                            expand_notes(&context, &mut events, invoke, spell, &params)?;
                        }
                        _ => bail!(
                            "{}: daemon `{}` cannot be invoked with spell `{spell_name}`",
                            invoke.span,
                            invoke.daemon
                        ),
                    }
                }
                None => {
                    let (id, semantic_path) =
                        event_identity(&rite.name, invoke.source_order, "once");
                    events.push(IrEvent {
                        id,
                        semantic_path,
                        kind: match daemon.kind {
                            DaemonKind::Sample => "trigger".to_string(),
                            DaemonKind::SampleKit => "trigger".to_string(),
                            DaemonKind::SawSub => "note".to_string(),
                            DaemonKind::Drone | DaemonKind::Swarm => "continuous".to_string(),
                            DaemonKind::NoiseBurst | DaemonKind::MetalHit => "trigger".to_string(),
                        },
                        time_beats: rite_start,
                        duration_beats: if matches!(
                            daemon.kind,
                            DaemonKind::Drone | DaemonKind::Swarm
                        ) {
                            duration_beats
                        } else {
                            invoke.every.map(|duration| duration.beats).unwrap_or(0.25)
                        },
                        daemon: invoke.daemon.clone(),
                        velocity: 1.0,
                        pitch: root_pitch(&params)?,
                        params,
                        source: source_for(input, invoke),
                        source_order: invoke.source_order,
                    });
                }
            }
        }
        cursor_beats = cursor_beats.max(rite_start + duration_beats);
    }

    events.sort_by(|a, b| {
        a.time_beats
            .total_cmp(&b.time_beats)
            .then(a.source_order.cmp(&b.source_order))
            .then(a.kind.cmp(&b.kind))
            .then(a.id.cmp(&b.id))
    });

    let random_streams = random_streams_for(&working);
    let circles = ir_circles(input, &working);
    let evoke_wav = working.evoke_wav;
    let ir = Ir {
        ir_version: "0.1".to_string(),
        language: "0.1".to_string(),
        working: working.name,
        tempo_bpm: working.tempo_bpm,
        meter: [working.meter.0, working.meter.1],
        seed: working.seed,
        random_streams,
        duration_beats: cursor_beats,
        circles,
        daemons: working
            .daemons
            .iter()
            .map(|daemon| IrDaemon {
                id: daemon.name.clone(),
                kind: match daemon.kind {
                    DaemonKind::Sample => "sample".to_string(),
                    DaemonKind::SampleKit => "samplekit".to_string(),
                    DaemonKind::SawSub => "saw_sub".to_string(),
                    DaemonKind::Drone => "drone".to_string(),
                    DaemonKind::NoiseBurst => "noise_burst".to_string(),
                    DaemonKind::Swarm => "swarm".to_string(),
                    DaemonKind::MetalHit => "metal_hit".to_string(),
                },
                sample: daemon.sample_path.clone(),
                params: merged_params(&daemon.params, &[]),
                source: source_for_span(input, daemon.span),
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
                transforms: spell
                    .transforms
                    .iter()
                    .map(pattern_transform_label)
                    .collect(),
                source: source_for_span(input, spell.span),
            })
            .collect(),
        rites,
        render_targets: vec![IrRenderTarget {
            id: "wav".to_string(),
            kind: "wav".to_string(),
            path: evoke_wav.clone(),
            source: source_for_span(input, working.evoke_span),
        }],
        control_events,
        events,
    };

    Ok(CompiledWorking {
        evoke_wav: PathBuf::from(evoke_wav),
        project_root: project_root.to_path_buf(),
        sample_root: project_root.join(&config.sample_dir),
        render_root: project_root.join(&config.render_dir),
        build_root: project_root.join(&config.build_dir),
        render_backend: "rust".to_string(),
        sample_rate: 48_000,
        bit_depth: 24,
        ir,
    })
}

fn random_streams_for(working: &Working) -> Vec<IrRandomStream> {
    let mut streams = Vec::new();
    let working_path = format!("working:{}", working.name);
    streams.push(random_stream(&working.seed, "working", &working_path));
    for spell in &working.spells {
        let path = format!("{working_path}/spell:{}", spell.name);
        streams.push(random_stream(
            &working.seed,
            &format!("spell:{}", spell.name),
            &path,
        ));
    }
    streams
}

fn random_stream(seed: &str, id: &str, semantic_path: &str) -> IrRandomStream {
    IrRandomStream {
        id: id.to_string(),
        semantic_path: semantic_path.to_string(),
        seed_hash: format!("{:016x}", stable_hash(&format!("{seed}:{semantic_path}"))),
    }
}

fn validate_invokes(
    working: &Working,
    daemon_map: &BTreeMap<&str, &crate::parser::Daemon>,
    spell_map: &BTreeMap<&str, &crate::parser::Spell>,
) -> Result<()> {
    let mut errors = Vec::new();
    for rite in &working.rites {
        for invoke in &rite.invokes {
            if let Some(every) = invoke.every
                && every.beats <= 0.0
            {
                errors.push(format!(
                    "{}: `every` duration must be positive",
                    invoke.span
                ));
            }
            let Some(daemon) = daemon_map.get(invoke.daemon.as_str()) else {
                errors.push(
                    unresolved_name(
                        "daemon",
                        &invoke.daemon,
                        daemon_map.keys().copied(),
                        invoke.span,
                    )
                    .to_string(),
                );
                continue;
            };
            if let Err(error) = validate_params(&invoke.daemon, daemon.kind.clone(), &invoke.params)
            {
                errors.push(format!("{}: {error}", invoke.span));
            }
            if let Some(spell_name) = &invoke.spell {
                let Some(spell) = spell_map.get(spell_name.as_str()) else {
                    errors.push(
                        unresolved_name(
                            "spell",
                            spell_name,
                            spell_map.keys().copied(),
                            invoke.span,
                        )
                        .to_string(),
                    );
                    continue;
                };
                if !matches!(
                    (&daemon.kind, &spell.kind),
                    (DaemonKind::Sample, PatternKind::Rhythm)
                        | (DaemonKind::SampleKit, PatternKind::Rhythm)
                        | (DaemonKind::NoiseBurst, PatternKind::Rhythm)
                        | (DaemonKind::MetalHit, PatternKind::Rhythm)
                        | (DaemonKind::SawSub, PatternKind::Notes)
                        | (DaemonKind::Swarm, PatternKind::Notes)
                ) {
                    errors.push(format!(
                        "{}: daemon `{}` cannot be invoked with spell `{spell_name}`",
                        invoke.span, invoke.daemon
                    ));
                }
            }
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("\n"));
    }
    Ok(())
}

fn validate_circles(working: &Working) -> Result<()> {
    let mut parents = BTreeMap::new();
    parents.insert("master", None::<&str>);
    for circle in &working.circles {
        parents.insert(
            circle.name.as_str(),
            circle.parent.as_deref().or(Some("master")),
        );
    }
    for circle in &working.circles {
        for ward in &circle.wards {
            validate_ward(ward)?;
        }
        if let Some(parent) = circle.parent.as_deref()
            && !parents.contains_key(parent)
        {
            bail!("{}: unresolved circle `{parent}`", circle.span);
        }
        let mut seen = HashSet::new();
        let mut cursor = circle.name.as_str();
        while let Some(Some(parent)) = parents.get(cursor) {
            if !seen.insert(cursor) {
                bail!(
                    "{}: routing cycle involving circle `{}`",
                    circle.span,
                    circle.name
                );
            }
            cursor = parent;
        }
    }
    for daemon in &working.daemons {
        for param in &daemon.params {
            if param.name == "out"
                && let Value::String(circle) = &param.value
                && !parents.contains_key(circle.as_str())
            {
                bail!("{}: unresolved circle `{circle}`", daemon.span);
            }
        }
    }
    Ok(())
}

fn ir_circles(input: &Path, working: &Working) -> Vec<IrCircle> {
    let mut circles = vec![IrCircle {
        id: "master".to_string(),
        parent: None,
        wards: Vec::new(),
        source: source_for_span(input, working.evoke_span),
    }];
    circles.extend(working.circles.iter().map(|circle| {
        IrCircle {
            id: circle.name.clone(),
            parent: circle.parent.clone().or_else(|| Some("master".to_string())),
            wards: circle
                .wards
                .iter()
                .map(|ward| IrWard {
                    kind: ward.kind.clone(),
                    param: ward.param.clone(),
                    value: ward.value,
                    source: source_for_span(input, ward.span),
                })
                .collect(),
            source: source_for_span(input, circle.span),
        }
    }));
    circles
}

fn validate_ward(ward: &crate::parser::Ward) -> Result<()> {
    match (ward.kind.as_str(), ward.param.as_str()) {
        ("limiter", "ceiling") if ward.value <= 0.0 => Ok(()),
        ("limiter", "ceiling") => {
            bail!("{}: limiter ceiling must be <= 0 dB", ward.span);
        }
        _ => bail!(
            "{}: unsupported ward `{} {}`",
            ward.span,
            ward.kind,
            ward.param
        ),
    }
}

fn ranges_overlap(left_start: f64, left_end: f64, right_start: f64, right_end: f64) -> bool {
    left_start < right_end && right_start < left_end
}

fn validate_control_target(target: &str, span: crate::lexer::Span) -> Result<()> {
    if matches!(
        target,
        "tension" | "density" | "instability" | "harshness" | "spaciousness" | "degradation"
    ) {
        Ok(())
    } else {
        bail!("{span}: unsupported control target `{target}`");
    }
}

fn validate_automation_curve(
    curve: &AutomationCurve,
    from: f64,
    to: f64,
    span: crate::lexer::Span,
) -> Result<()> {
    if matches!(curve, AutomationCurve::Exponential) && (from <= 0.0 || to <= 0.0) {
        bail!("{span}: exponential automation endpoints must be positive");
    }
    Ok(())
}

fn automation_curve_label(curve: &AutomationCurve) -> &'static str {
    match curve {
        AutomationCurve::Linear => "linear",
        AutomationCurve::Exponential => "exponential",
        AutomationCurve::Stepped => "stepped",
    }
}

fn truncate_continuous_events(events: &mut [IrEvent], daemon: &str, banish_time: f64) {
    for event in events {
        if event.daemon == daemon
            && event.kind == "continuous"
            && event.time_beats < banish_time
            && event.time_beats + event.duration_beats > banish_time
        {
            event.duration_beats = banish_time - event.time_beats;
        }
    }
}

struct ExpansionContext<'a> {
    input: &'a Path,
    rite_name: &'a str,
    seed: &'a str,
    rite_start: f64,
    rite_duration: f64,
}

impl<'a> ExpansionContext<'a> {
    fn new(
        input: &'a Path,
        rite_name: &'a str,
        seed: &'a str,
        rite_start: f64,
        rite_duration: f64,
    ) -> Self {
        Self {
            input,
            rite_name,
            seed,
            rite_start,
            rite_duration,
        }
    }
}

fn expand_rhythm(
    context: &ExpansionContext<'_>,
    events: &mut Vec<IrEvent>,
    invoke: &crate::parser::Invoke,
    spell: &crate::parser::Spell,
    params: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let mut steps = rhythm_steps(&spell.body)?;
    apply_transforms(&mut steps, &spell.transforms)?;
    let step_duration = invoke
        .every
        .or_else(|| transform_every(&spell.transforms))
        .map(|duration| duration.beats)
        .unwrap_or(0.25);
    let total_steps = (context.rite_duration / step_duration).ceil() as usize;
    for absolute_step in 0..total_steps {
        let time = absolute_step as f64 * step_duration;
        if time >= context.rite_duration {
            break;
        }
        let (id, semantic_path) = event_identity(
            context.rite_name,
            invoke.source_order,
            &absolute_step.to_string(),
        );
        if let Some(mut velocity) = stochastic_rhythm_step(
            context.seed,
            &semantic_path,
            absolute_step,
            steps[absolute_step % steps.len()],
            &spell.transforms,
        )? {
            let event_time = humanized_time(
                context.seed,
                &semantic_path,
                absolute_step,
                context.rite_start,
                time,
                context.rite_duration,
                &spell.transforms,
            )?;
            velocity = velocity.clamp(0.0, 4.0);
            events.push(IrEvent {
                id,
                semantic_path,
                kind: "trigger".to_string(),
                time_beats: event_time,
                duration_beats: step_duration,
                daemon: invoke.daemon.clone(),
                velocity,
                pitch: None,
                params: params.clone(),
                source: source_for(context.input, invoke),
                source_order: invoke.source_order,
            });
        }
    }
    Ok(())
}

fn expand_notes(
    context: &ExpansionContext<'_>,
    events: &mut Vec<IrEvent>,
    invoke: &crate::parser::Invoke,
    spell: &crate::parser::Spell,
    params: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let mut steps = note_steps(&spell.body)?;
    apply_transforms(&mut steps, &spell.transforms)?;
    let step_duration = invoke
        .every
        .or_else(|| transform_every(&spell.transforms))
        .map(|duration| duration.beats)
        .unwrap_or(0.5);
    let total_steps = (context.rite_duration / step_duration).ceil() as usize;
    for absolute_step in 0..total_steps {
        let time = absolute_step as f64 * step_duration;
        if time >= context.rite_duration {
            break;
        }
        let (id, semantic_path) = event_identity(
            context.rite_name,
            invoke.source_order,
            &absolute_step.to_string(),
        );
        if let Some(pitch_name) = stochastic_note_step(
            context.seed,
            &semantic_path,
            absolute_step,
            steps[absolute_step % steps.len()].as_ref(),
            &spell.transforms,
        )? {
            let velocity = stochastic_velocity(
                context.seed,
                &semantic_path,
                absolute_step,
                1.0,
                &spell.transforms,
            )?;
            let event_time = humanized_time(
                context.seed,
                &semantic_path,
                absolute_step,
                context.rite_start,
                time,
                context.rite_duration,
                &spell.transforms,
            )?;
            events.push(IrEvent {
                id,
                semantic_path,
                kind: "note".to_string(),
                time_beats: event_time,
                duration_beats: step_duration,
                daemon: invoke.daemon.clone(),
                velocity,
                pitch: Some(IrPitch {
                    name: pitch_name.clone(),
                    midi: pitch_to_midi(pitch_name)?,
                }),
                params: params.clone(),
                source: source_for(context.input, invoke),
                source_order: invoke.source_order,
            });
        }
    }
    Ok(())
}

fn rhythm_steps(body: &str) -> Result<Vec<Option<f64>>> {
    if let Some((pulses, step_count, rotate)) = parse_euclid(body)? {
        let mut steps = euclid_steps(pulses, step_count)?;
        rotate_steps(&mut steps, rotate);
        return Ok(steps);
    }

    let mut steps = Vec::new();
    for ch in body.chars() {
        match ch {
            'x' => steps.push(Some(1.0)),
            'X' => steps.push(Some(1.25)),
            'g' => steps.push(Some(0.45)),
            '-' => steps.push(None),
            ' ' => {}
            other => bail!("unsupported rhythm pattern character `{other}`"),
        }
    }
    if steps.is_empty() {
        bail!("rhythm pattern cannot be empty");
    }
    Ok(steps)
}

fn apply_transforms<T: Clone>(steps: &mut Vec<T>, transforms: &[PatternTransform]) -> Result<()> {
    for transform in transforms {
        match transform {
            PatternTransform::Rotate(amount) => rotate_steps(steps, *amount),
            PatternTransform::Reverse => steps.reverse(),
            PatternTransform::Repeat(count) => {
                if *count == 0 {
                    bail!("pattern repeat count must be positive");
                }
                let original = steps.clone();
                for _ in 1..*count {
                    steps.extend(original.iter().cloned());
                }
            }
            PatternTransform::Every(duration) if duration.beats <= 0.0 => {
                bail!("pattern every duration must be positive");
            }
            PatternTransform::Every(_) => {}
            PatternTransform::Degrade(amount)
            | PatternTransform::Humanize(amount)
            | PatternTransform::Mutate(amount)
                if !(0.0..=1.0).contains(amount) =>
            {
                bail!("stochastic pattern transform amounts must be in [0, 1]");
            }
            PatternTransform::Degrade(_)
            | PatternTransform::Humanize(_)
            | PatternTransform::Mutate(_) => {}
            PatternTransform::VelocityRange(min, max) if *min < 0.0 || min > max => {
                bail!("velocity range must be non-negative and ordered");
            }
            PatternTransform::VelocityRange(_, _) => {}
        }
    }
    Ok(())
}

fn transform_every(transforms: &[PatternTransform]) -> Option<crate::parser::Duration> {
    transforms.iter().find_map(|transform| match transform {
        PatternTransform::Every(duration) => Some(*duration),
        _ => None,
    })
}

fn pattern_transform_label(transform: &PatternTransform) -> String {
    match transform {
        PatternTransform::Rotate(steps) => format!("rotate({steps})"),
        PatternTransform::Reverse => "reverse()".to_string(),
        PatternTransform::Repeat(count) => format!("repeat({count})"),
        PatternTransform::Every(duration) => format!("every({})", duration.beats),
        PatternTransform::Degrade(amount) => format!("degrade({amount})"),
        PatternTransform::Humanize(amount) => format!("humanize({amount})"),
        PatternTransform::Mutate(amount) => format!("mutate({amount})"),
        PatternTransform::VelocityRange(min, max) => format!("velocity(rand({min}, {max}))"),
    }
}

fn stochastic_rhythm_step(
    seed: &str,
    semantic_path: &str,
    step: usize,
    value: Option<f64>,
    transforms: &[PatternTransform],
) -> Result<Option<f64>> {
    let mut value = value;
    for transform in transforms {
        match transform {
            PatternTransform::Degrade(amount)
                if value.is_some()
                    && random_unit(seed, semantic_path, step, "degrade") < *amount =>
            {
                value = None;
            }
            PatternTransform::Mutate(amount)
                if random_unit(seed, semantic_path, step, "mutate") < *amount =>
            {
                value = if value.is_some() { None } else { Some(1.0) };
            }
            _ => {}
        }
    }
    value
        .map(|velocity| stochastic_velocity(seed, semantic_path, step, velocity, transforms))
        .transpose()
}

fn stochastic_note_step<'a>(
    seed: &str,
    semantic_path: &str,
    step: usize,
    value: Option<&'a String>,
    transforms: &[PatternTransform],
) -> Result<Option<&'a String>> {
    let mut value = value;
    for transform in transforms {
        match transform {
            PatternTransform::Degrade(amount) | PatternTransform::Mutate(amount)
                if value.is_some()
                    && random_unit(seed, semantic_path, step, "note_drop") < *amount =>
            {
                value = None;
            }
            _ => {}
        }
    }
    Ok(value)
}

fn stochastic_velocity(
    seed: &str,
    semantic_path: &str,
    step: usize,
    base: f64,
    transforms: &[PatternTransform],
) -> Result<f64> {
    let mut velocity = base;
    for transform in transforms {
        if let PatternTransform::VelocityRange(min, max) = transform {
            if *min < 0.0 || min > max {
                bail!("velocity range must be non-negative and ordered");
            }
            let amount = random_unit(seed, semantic_path, step, "velocity");
            velocity *= min + (max - min) * amount;
        }
    }
    Ok(velocity)
}

fn humanized_time(
    seed: &str,
    semantic_path: &str,
    step: usize,
    rite_start: f64,
    step_time: f64,
    rite_duration: f64,
    transforms: &[PatternTransform],
) -> Result<f64> {
    let mut offset = 0.0;
    for transform in transforms {
        if let PatternTransform::Humanize(amount) = transform {
            if !(0.0..=1.0).contains(amount) {
                bail!("stochastic pattern transform amounts must be in [0, 1]");
            }
            let unit = random_unit(seed, semantic_path, step, "humanize");
            offset += (unit * 2.0 - 1.0) * amount;
        }
    }
    Ok(rite_start + (step_time + offset).clamp(0.0, rite_duration.max(0.0)))
}

fn random_unit(seed: &str, semantic_path: &str, step: usize, channel: &str) -> f64 {
    let hash = stable_hash(&format!("{seed}:{semantic_path}:{step}:{channel}"));
    hash as f64 / u64::MAX as f64
}

fn parse_euclid(body: &str) -> Result<Option<(u32, u32, i32)>> {
    let Some(rest) = body.strip_prefix("euclid(") else {
        return Ok(None);
    };
    let Some((args, transform)) = rest.split_once(')') else {
        bail!("euclid rhythm must use `euclid(pulses, steps)`");
    };
    if !transform.is_empty() && !transform.starts_with(".rotate(") {
        bail!("unsupported euclid transform `{transform}`");
    }
    let Some((pulses, steps)) = args.split_once(',') else {
        bail!("euclid rhythm must use `euclid(pulses, steps)`");
    };
    let rotate = if transform.is_empty() {
        0
    } else {
        transform
            .strip_prefix(".rotate(")
            .and_then(|value| value.strip_suffix(')'))
            .ok_or_else(|| anyhow::anyhow!("euclid rotate must use `.rotate(steps)`"))?
            .trim()
            .parse()?
    };
    Ok(Some((
        pulses.trim().parse()?,
        steps.trim().parse()?,
        rotate,
    )))
}

fn euclid_steps(pulses: u32, step_count: u32) -> Result<Vec<Option<f64>>> {
    if step_count == 0 {
        bail!("euclid rhythm must have at least one step");
    }
    if pulses > step_count {
        bail!("euclid rhythm pulses cannot exceed steps");
    }

    let steps = (0..step_count)
        .map(|step| {
            if pulses > 0 && (step * pulses) % step_count < pulses {
                Some(1.0)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Ok(steps)
}

fn rotate_steps<T>(steps: &mut [T], rotate: i32) {
    if steps.is_empty() {
        return;
    }
    let len = steps.len() as i32;
    let amount = rotate.rem_euclid(len) as usize;
    steps.rotate_right(amount);
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

fn unresolved_name<'a>(
    kind: &str,
    name: &str,
    candidates: impl Iterator<Item = &'a str>,
    span: crate::lexer::Span,
) -> anyhow::Error {
    let candidates = candidates.collect::<Vec<_>>();
    let suggestion = nearest_name(name, &candidates)
        .map(|candidate| format!("; did you mean `{candidate}`?"))
        .unwrap_or_default();
    anyhow::anyhow!("{span}: unresolved {kind} `{name}`{suggestion}")
}

fn nearest_name<'a>(name: &str, candidates: &'a [&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|candidate| (*candidate, edit_distance(name, candidate)))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(candidate, _)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    let mut current = vec![0; previous.len()];
    for (left_index, left_ch) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_ch) in right.chars().enumerate() {
            let substitution = previous[right_index] + usize::from(left_ch != right_ch);
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            current[right_index + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.chars().count()]
}

fn validate_sample_path(project_root: &Path, config: &ProjectConfig, sample: &str) -> Result<()> {
    if sample.starts_with('~') || sample.contains('*') || sample.contains("://") {
        bail!("sample path `{sample}` is not valid in language 0.1");
    }
    let direct_path = project_root.join(sample);
    let manifest_path = project_root.join(&config.sample_dir).join(sample);
    if !direct_path.exists() && !manifest_path.exists() {
        bail!("sample file `{}` does not exist", manifest_path.display());
    }
    Ok(())
}

fn validate_samplekit_path(
    project_root: &Path,
    config: &ProjectConfig,
    sample_dir: &str,
) -> Result<()> {
    if sample_dir.starts_with('~') || sample_dir.contains('*') || sample_dir.contains("://") {
        bail!("samplekit path `{sample_dir}` is not valid in language 0.1");
    }
    let direct_path = project_root.join(sample_dir);
    let manifest_path = project_root.join(&config.sample_dir).join(sample_dir);
    let path = if direct_path.exists() {
        direct_path
    } else {
        manifest_path
    };
    if !path.is_dir() {
        bail!("samplekit directory `{}` does not exist", path.display());
    }
    let has_wav = fs::read_dir(&path)
        .with_context(|| format!("failed to read `{}`", path.display()))?
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("wav"));
    if !has_wav {
        bail!(
            "samplekit directory `{}` contains no .wav files",
            path.display()
        );
    }
    Ok(())
}

pub fn validate_output_path(out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        && parent.exists()
        && !parent.is_dir()
    {
        bail!("output parent `{}` is not a directory", parent.display());
    }
    Ok(())
}

fn validate_working_header(working: &Working) -> Result<()> {
    if working.tempo_bpm <= 0.0 {
        bail!("tempo must be positive");
    }
    if working.meter.0 == 0 || working.meter.1 == 0 {
        bail!("meter must use positive numerator and denominator");
    }
    if !matches!(working.meter.1, 1 | 2 | 4 | 8 | 16 | 32) {
        bail!("unsupported meter denominator `{}`", working.meter.1);
    }
    Ok(())
}

fn validate_params(owner: &str, kind: DaemonKind, params: &[crate::parser::Param]) -> Result<()> {
    for param in params {
        let allowed = match kind {
            DaemonKind::Sample | DaemonKind::SampleKit => matches!(
                param.name.as_str(),
                "gain"
                    | "pan"
                    | "tune"
                    | "highpass"
                    | "lowpass"
                    | "start"
                    | "end"
                    | "normalize"
                    | "out"
            ),
            DaemonKind::SawSub => {
                matches!(
                    param.name.as_str(),
                    "gain"
                        | "pan"
                        | "cutoff"
                        | "drive"
                        | "attack"
                        | "decay"
                        | "sustain"
                        | "release"
                        | "detune"
                        | "sub"
                        | "resonance"
                        | "out"
                )
            }
            DaemonKind::Drone => {
                matches!(
                    param.name.as_str(),
                    "gain" | "pan" | "cutoff" | "drive" | "root" | "out"
                )
            }
            DaemonKind::NoiseBurst => {
                matches!(
                    param.name.as_str(),
                    "gain" | "pan" | "highpass" | "lowpass" | "drive" | "out"
                )
            }
            DaemonKind::Swarm => {
                matches!(
                    param.name.as_str(),
                    "gain" | "pan" | "cutoff" | "drive" | "root" | "voices" | "spread" | "out"
                )
            }
            DaemonKind::MetalHit => {
                matches!(
                    param.name.as_str(),
                    "gain" | "pan" | "root" | "drive" | "decay" | "out"
                )
            }
        };
        if !allowed {
            bail!("`{}` does not support parameter `{}`", owner, param.name);
        }
        validate_param_value(owner, &param.name, &param.value)?;
    }
    Ok(())
}

fn validate_param_value(owner: &str, name: &str, value: &Value) -> Result<()> {
    if name == "root" {
        if matches!(value, Value::Pitch(_)) {
            return Ok(());
        }
        bail!("`{owner}` parameter `root` must be a pitch");
    }
    if name == "out" {
        if matches!(value, Value::String(_)) {
            return Ok(());
        }
        bail!("`{owner}` parameter `out` must be a circle name");
    }
    if name == "normalize" {
        if matches!(value, Value::String(value) if value == "on" || value == "off") {
            return Ok(());
        }
        bail!("`{owner}` parameter `normalize` must be `on` or `off`");
    }
    let number = match value {
        Value::Number(number) => *number,
        _ => bail!("`{owner}` parameter `{name}` must be numeric"),
    };
    match name {
        "pan" if !(-1.0..=1.0).contains(&number) => {
            bail!("`{owner}` parameter `pan` must be in [-1, 1]");
        }
        "drive" if !(0.0..=1.0).contains(&number) => {
            bail!("`{owner}` parameter `drive` must be in [0, 1]");
        }
        "cutoff" | "highpass" | "lowpass" if number <= 0.0 => {
            bail!("`{owner}` parameter `{name}` must be positive");
        }
        "voices" if number < 1.0 || number.fract() != 0.0 => {
            bail!("`{owner}` parameter `voices` must be a positive integer");
        }
        "spread" if number <= 0.0 => {
            bail!("`{owner}` parameter `{name}` must be positive");
        }
        "start" | "end" if number < 0.0 => {
            bail!("`{owner}` parameter `{name}` must be non-negative");
        }
        "attack" | "decay" | "release" if number < 0.0 => {
            bail!("`{owner}` parameter `{name}` must be non-negative");
        }
        "sustain" | "sub" if !(0.0..=1.0).contains(&number) => {
            bail!("`{owner}` parameter `{name}` must be in [0, 1]");
        }
        "detune" if number < 0.0 => {
            bail!("`{owner}` parameter `detune` must be non-negative");
        }
        "resonance" if !(0.05..=1.0).contains(&number) => {
            bail!("`{owner}` parameter `resonance` must be in [0.05, 1]");
        }
        _ => {}
    }
    Ok(())
}

fn root_pitch(params: &BTreeMap<String, serde_json::Value>) -> Result<Option<IrPitch>> {
    let Some(root) = params.get("root").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    Ok(Some(IrPitch {
        name: root.to_string(),
        midi: pitch_to_midi(root)?,
    }))
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
        "start" => "start_seconds",
        "end" => "end_seconds",
        "attack" => "attack_seconds",
        "decay" => "decay_seconds",
        "release" => "release_seconds",
        "tune" => "tune_semitones",
        "detune" => "detune_cents",
        "sub" => "sub_level",
        "spread" => "spread_cents",
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
    source_for_span(input, invoke.span)
}

fn source_for_span(input: &Path, span: crate::lexer::Span) -> IrSource {
    IrSource {
        file: input.display().to_string(),
        line: span.line,
        column: span.column,
    }
}

fn event_identity(rite: &str, invoke_order: usize, step: &str) -> (String, String) {
    let semantic_path = format!("rite:{rite}/invoke:{invoke_order}/step:{step}");
    let id = format!("evt_{:016x}", stable_hash(&semantic_path));
    (id, semantic_path)
}

fn control_identity(rite: &str, automation_index: usize) -> (String, String) {
    let semantic_path = format!("rite:{rite}/control:{automation_index}");
    let id = format!("ctrl_{:016x}", stable_hash(&semantic_path));
    (id, semantic_path)
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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

        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
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

    #[test]
    fn expands_rhythm_accents_and_ghosts_to_velocity() {
        let root =
            std::env::temp_dir().join(format!("malison-velocity-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Velocity Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell kicks = pattern "Xg--"

  rite main bars 1 {
    invoke kick with kicks every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let velocities = compiled
            .ir
            .events
            .iter()
            .take(2)
            .map(|event| event.velocity)
            .collect::<Vec<_>>();
        assert_eq!(velocities, vec![1.25, 0.45]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn expands_euclidean_rhythm_spells() {
        let root = std::env::temp_dir().join(format!("malison-euclid-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Euclid Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = euclid(3, 8)

  rite main bars 1 {
    invoke kick with hits every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let times = compiled
            .ir
            .events
            .iter()
            .map(|event| event.time_beats)
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.0, 0.75, 1.5, 2.0, 2.75, 3.5]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rotates_euclidean_rhythm_spells() {
        let root =
            std::env::temp_dir().join(format!("malison-euclid-rotate-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Euclid Rotate Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = euclid(3, 8).rotate(2)

  rite main bars 1 {
    invoke kick with hits every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let times = compiled
            .ir
            .events
            .iter()
            .map(|event| event.time_beats)
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.0, 0.5, 1.25, 2.0, 2.5, 3.25]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validates_euclidean_rhythm_edges() {
        assert_eq!(euclid_steps(0, 4).unwrap(), vec![None, None, None, None]);
        assert_eq!(
            euclid_steps(4, 4).unwrap(),
            vec![Some(1.0), Some(1.0), Some(1.0), Some(1.0)]
        );
        assert!(
            euclid_steps(1, 0)
                .unwrap_err()
                .to_string()
                .contains("at least one step")
        );
        assert!(
            euclid_steps(5, 4)
                .unwrap_err()
                .to_string()
                .contains("cannot exceed steps")
        );
    }

    #[test]
    fn applies_string_pattern_transforms() {
        let root =
            std::env::temp_dir().join(format!("malison-transform-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Transform Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = pattern "x---".rotate(1).repeat(2)

  rite main bars 1 {
    invoke kick with hits every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let times = compiled
            .ir
            .events
            .iter()
            .map(|event| event.time_beats)
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.25, 1.25, 2.25, 3.25]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pattern_every_supplies_default_step_duration() {
        let root = std::env::temp_dir().join(format!("malison-every-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Every Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = pattern "x-".every(1/8)

  rite main bars 1 {
    invoke kick with hits
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let times = compiled
            .ir
            .events
            .iter()
            .map(|event| event.time_beats)
            .collect::<Vec<_>>();
        assert_eq!(times, vec![0.0, 1.0, 2.0, 3.0]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn stochastic_transforms_are_seeded_and_bounded() {
        let root =
            std::env::temp_dir().join(format!("malison-stochastic-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Stochastic Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = pattern "xxxx".degrade(0.25).humanize(0.1).velocity(rand(0.5, 0.75))

  rite main bars 1 {
    invoke kick with hits every 1/4
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let first_pass = compiled
            .ir
            .events
            .iter()
            .map(|event| (event.time_beats, event.velocity))
            .collect::<Vec<_>>();
        assert!(!first_pass.is_empty());
        assert!(
            first_pass
                .iter()
                .all(|(time, velocity)| (0.0..=4.0).contains(time)
                    && (0.5..=0.75).contains(velocity))
        );

        let working = parse_source(&path, source).unwrap();
        let compiled_again =
            compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let second_pass = compiled_again
            .ir
            .events
            .iter()
            .map(|event| (event.time_beats, event.velocity))
            .collect::<Vec<_>>();
        assert_eq!(first_pass, second_pass);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn schedules_explicit_rites_and_rejects_overlaps() {
        let root =
            std::env::temp_dir().join(format!("malison-arrangement-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Arrangement Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = pattern "x---"

  rite intro bars 1 {
    invoke kick with hits every 1/16
  }

  rite drop at bar 3 bars 1 {
    invoke kick with hits every 1/16
  }

  rite texture at 0:02 bars 1 layer {
    invoke kick with hits every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let starts = compiled
            .ir
            .rites
            .iter()
            .map(|rite| rite.start_beats)
            .collect::<Vec<_>>();
        assert_eq!(starts, vec![0.0, 8.0, 4.0]);
        assert!(compiled.ir.rites[2].layer);

        let overlapping = source.replace("rite drop at bar 3", "rite drop at bar 1");
        fs::write(&path, &overlapping).unwrap();
        let working = parse_source(&path, &overlapping).unwrap();
        let error = compile_events(&path, &root, &ProjectConfig::default(), working)
            .unwrap_err()
            .to_string();
        assert!(error.contains("overlaps rite `intro`"));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lowers_drone_invocations_to_continuous_events() {
        let root = std::env::temp_dir().join(format!("malison-drone-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Drone Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon bed = drone { root F1 gain -18 cutoff 700 drive 0.2 }

  rite main bars 2 {
    invoke bed
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        assert_eq!(compiled.ir.events[0].kind, "continuous");
        assert_eq!(compiled.ir.events[0].duration_beats, 8.0);
        assert_eq!(compiled.ir.events[0].pitch.as_ref().unwrap().name, "F1");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lowers_rite_automation_to_control_events() {
        let root =
            std::env::temp_dir().join(format!("malison-automation-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Automation Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell hits = pattern "x---"

  rite main bars 2 {
    invoke kick with hits every 1/16
    raise tension 0.2 -> 0.8 curve exponential
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        assert_eq!(compiled.ir.control_events.len(), 1);
        assert_eq!(compiled.ir.control_events[0].target, "tension");
        assert_eq!(compiled.ir.control_events[0].curve, "exponential");
        assert_eq!(compiled.ir.control_events[0].duration_beats, 8.0);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lowers_banish_to_lifecycle_event_and_truncates_continuous_sources() {
        let root = std::env::temp_dir().join(format!("malison-banish-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();

        let source = r#"
language 0.1

working "Banish Test" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon bed = drone { root F1 gain -18 }
  daemon hit = noise_burst

  rite bed_layer at bar 1 bars 4 layer {
    invoke bed
  }

  rite cut at bar 3 bars 1 layer {
    banish bed
    invoke hit
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        let continuous = compiled
            .ir
            .events
            .iter()
            .find(|event| event.kind == "continuous")
            .unwrap();
        assert_eq!(continuous.duration_beats, 8.0);
        assert!(
            compiled
                .ir
                .events
                .iter()
                .any(|event| event.kind == "banish" && event.daemon == "bed")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validates_circle_routing() {
        let root = std::env::temp_dir().join(format!("malison-circle-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("samples/kick.wav"), b"not really wav").unwrap();

        let source = r#"
language 0.1

working "Circle Test" {
  tempo 120
  meter 4/4
  seed "seed"

  circle drums -> master {
    ward limiter ceiling -1
  }
  daemon kick = sample "samples/kick.wav" { out drums }
  spell hits = pattern "x---"

  rite main bars 1 {
    invoke kick with hits every 1/16
  }

  evoke wav "renders/test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let compiled = compile_events(&path, &root, &ProjectConfig::default(), working).unwrap();
        assert_eq!(compiled.ir.circles.len(), 2);
        assert_eq!(compiled.ir.circles[1].parent.as_deref(), Some("master"));
        assert_eq!(compiled.ir.circles[1].wards[0].kind, "limiter");

        let bad = source.replace("out drums", "out nowhere");
        fs::write(&path, &bad).unwrap();
        let working = parse_source(&path, &bad).unwrap();
        let error = compile_events(&path, &root, &ProjectConfig::default(), working)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unresolved circle `nowhere`"));

        fs::remove_dir_all(&root).unwrap();
    }
}
