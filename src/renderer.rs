use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::compiler::{CompiledWorking, IrDaemon, IrEvent};

const RENDER_TAIL_SECONDS: f64 = 2.0;

pub fn render_wav(
    compiled: &CompiledWorking,
    out_path: &Path,
    sample_rate: u32,
    bit_depth: u16,
) -> Result<()> {
    if !matches!(bit_depth, 16 | 24 | 32) {
        bail!("unsupported bit depth `{bit_depth}`; expected 16, 24, or 32");
    }

    let duration_seconds =
        beats_to_seconds(compiled.ir.duration_beats, compiled.ir.tempo_bpm) + RENDER_TAIL_SECONDS;
    let frame_count = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut buffer = vec![[0.0_f32; 2]; frame_count];

    let daemons = compiled
        .ir
        .daemons
        .iter()
        .map(|daemon| (daemon.id.as_str(), daemon))
        .collect::<BTreeMap<_, _>>();

    for event in &compiled.ir.events {
        let daemon = daemons
            .get(event.daemon.as_str())
            .ok_or_else(|| anyhow::anyhow!("event references unknown daemon `{}`", event.daemon))?;
        match daemon.kind.as_str() {
            "sample" => render_sample(compiled, daemon, event, sample_rate, &mut buffer)?,
            "saw_sub" => render_saw_sub(event, &compiled.ir.tempo_bpm, sample_rate, &mut buffer),
            other => bail!("unsupported daemon kind `{other}`"),
        }
    }

    if let Some(parent) = out_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }
    write_wav(out_path, sample_rate, bit_depth, &buffer)
}

pub fn render_supercollider(
    compiled: &CompiledWorking,
    out_path: &Path,
    sample_rate: u32,
    bit_depth: u16,
) -> Result<()> {
    if !matches!(bit_depth, 16 | 24 | 32) {
        bail!("unsupported bit depth `{bit_depth}`; expected 16, 24, or 32");
    }
    if let Some(parent) = out_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }

    let script = supercollider_script(compiled, out_path, sample_rate, bit_depth)?;
    let script_path = temp_script_path();
    fs::write(&script_path, script)
        .with_context(|| format!("failed to write `{}`", script_path.display()))?;

    let output = Command::new("sclang")
        .arg(&script_path)
        .env("QT_QPA_PLATFORM", "offscreen")
        .output()
        .context("failed to run `sclang`; is SuperCollider installed?")?;

    let _ = fs::remove_file(&script_path);

    if !output.status.success() {
        bail!(
            "sclang failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !out_path.exists() {
        bail!(
            "SuperCollider completed but did not create `{}`\nstdout:\n{}\nstderr:\n{}",
            out_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub fn supercollider_script(
    compiled: &CompiledWorking,
    out_path: &Path,
    sample_rate: u32,
    bit_depth: u16,
) -> Result<String> {
    let mut daemons = BTreeMap::new();
    for daemon in &compiled.ir.daemons {
        daemons.insert(daemon.id.as_str(), daemon);
    }

    let mut sample_buffers = BTreeMap::new();
    let mut next_bufnum = 0;
    for daemon in &compiled.ir.daemons {
        if daemon.kind == "sample" {
            sample_buffers.insert(daemon.id.as_str(), next_bufnum);
            next_bufnum += 1;
        }
    }

    let mut score_lines = Vec::new();
    score_lines.push("[0.0, [\\d_recv, SynthDef(\\mal_sample, { |out=0, bufnum=0, amp=1, pan=0, rate=1| var sig; sig = PlayBuf.ar(1, bufnum, BufRateScale.kr(bufnum) * rate, doneAction:2); Out.ar(out, Pan2.ar(sig * amp, pan)); }).asBytes]]".to_string());
    score_lines.push("[0.0, [\\d_recv, SynthDef(\\mal_saw_sub, { |out=0, freq=55, dur=0.25, amp=0.3, pan=0, cutoff=1200, drive=0| var hold, env, sig, driven; hold = (dur - 0.19).max(0.001); env = EnvGen.kr(Env([0, 1, 0.65, 0.65, 0], [0.01, 0.18, hold, 0.08]), doneAction:2); sig = (Saw.ar(freq) * 0.72) + (Saw.ar(freq * 0.5) * 0.28); sig = RLPF.ar(sig, cutoff.clip(20, 20000), 0.35); driven = (sig * (1 + (drive * 12))).tanh; Out.ar(out, Pan2.ar(driven * env * amp, pan)); }).asBytes]]".to_string());

    for daemon in &compiled.ir.daemons {
        if daemon.kind == "sample" {
            let sample_path = daemon.sample.as_deref().ok_or_else(|| {
                anyhow::anyhow!("sample daemon `{}` has no sample path", daemon.id)
            })?;
            let path = compiled.project_root.join(sample_path);
            let bufnum = sample_buffers[daemon.id.as_str()];
            score_lines.push(format!(
                "[0.0, [\\b_allocRead, {}, {}]]",
                bufnum,
                sc_string(&path.display().to_string())
            ));
        }
    }

    let mut node_id = 1000;
    for event in &compiled.ir.events {
        let daemon = daemons
            .get(event.daemon.as_str())
            .ok_or_else(|| anyhow::anyhow!("event references unknown daemon `{}`", event.daemon))?;
        let time = beats_to_seconds(event.time_beats, compiled.ir.tempo_bpm);
        match daemon.kind.as_str() {
            "sample" => {
                let amp = db_to_amp(param_f64(&event.params, "gain_db").unwrap_or(0.0));
                let pan = param_f64(&event.params, "pan")
                    .unwrap_or(0.0)
                    .clamp(-1.0, 1.0);
                let tune = param_f64(&event.params, "tune_semitones").unwrap_or(0.0);
                let rate = 2.0_f64.powf(tune / 12.0);
                let bufnum = sample_buffers[event.daemon.as_str()];
                score_lines.push(format!(
                    "[{time:.6}, [\\s_new, \\mal_sample, {node_id}, 0, 1, \\bufnum, {bufnum}, \\amp, {amp:.8}, \\pan, {pan:.8}, \\rate, {rate:.8}]]"
                ));
                node_id += 1;
            }
            "saw_sub" => {
                if let Some(pitch) = &event.pitch {
                    let freq = midi_to_freq(pitch.midi);
                    let dur = beats_to_seconds(event.duration_beats, compiled.ir.tempo_bpm);
                    let amp = db_to_amp(param_f64(&event.params, "gain_db").unwrap_or(-10.0));
                    let pan = param_f64(&event.params, "pan")
                        .unwrap_or(0.0)
                        .clamp(-1.0, 1.0);
                    let cutoff = param_f64(&event.params, "cutoff_hz").unwrap_or(1200.0);
                    let drive = param_f64(&event.params, "drive")
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    score_lines.push(format!(
                        "[{time:.6}, [\\s_new, \\mal_saw_sub, {node_id}, 0, 1, \\freq, {freq:.8}, \\dur, {dur:.8}, \\amp, {amp:.8}, \\pan, {pan:.8}, \\cutoff, {cutoff:.8}, \\drive, {drive:.8}]]"
                    ));
                    node_id += 1;
                }
            }
            other => bail!("unsupported daemon kind `{other}`"),
        }
    }

    let duration =
        beats_to_seconds(compiled.ir.duration_beats, compiled.ir.tempo_bpm) + RENDER_TAIL_SECONDS;
    score_lines.push(format!("[{duration:.6}, [\\c_set, 0, 0]]"));

    Ok(format!(
        r#"(
var opts, score;
opts = ServerOptions.new.numOutputBusChannels_(2).sampleRate_({sample_rate});
score = Score([
  {}
]);
score.recordNRT(
  outputFilePath: {},
  sampleRate: {sample_rate},
  headerFormat: "WAV",
  sampleFormat: {},
  options: opts,
  duration: {duration:.6},
  action: {{ 0.exit }}
);
)"#,
        score_lines.join(",\n  "),
        sc_string(&out_path.display().to_string()),
        sc_string(sc_sample_format(bit_depth)?),
    ))
}

fn render_sample(
    compiled: &CompiledWorking,
    daemon: &IrDaemon,
    event: &IrEvent,
    sample_rate: u32,
    buffer: &mut [[f32; 2]],
) -> Result<()> {
    let sample_path = daemon
        .sample
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("sample daemon `{}` has no sample path", daemon.id))?;
    let path = compiled.project_root.join(sample_path);
    let sample = read_wav(&path, sample_rate)?;
    let start = seconds_to_frame(
        beats_to_seconds(event.time_beats, compiled.ir.tempo_bpm),
        sample_rate,
    );
    let gain = db_to_amp(param_f64(&event.params, "gain_db").unwrap_or(0.0)) as f32;
    let pan = param_f64(&event.params, "pan").unwrap_or(0.0) as f32;
    mix_frames(buffer, start, &sample, gain, pan);
    Ok(())
}

fn render_saw_sub(event: &IrEvent, tempo_bpm: &f64, sample_rate: u32, buffer: &mut [[f32; 2]]) {
    let Some(pitch) = &event.pitch else {
        return;
    };
    let start = seconds_to_frame(beats_to_seconds(event.time_beats, *tempo_bpm), sample_rate);
    let note_seconds = beats_to_seconds(event.duration_beats, *tempo_bpm);
    let release = 0.08_f64;
    let frames = ((note_seconds + release) * sample_rate as f64).ceil() as usize;
    let freq = 440.0_f32 * 2.0_f32.powf((pitch.midi as f32 - 69.0) / 12.0);
    let gain = db_to_amp(param_f64(&event.params, "gain_db").unwrap_or(-10.0)) as f32;
    let pan = param_f64(&event.params, "pan").unwrap_or(0.0) as f32;
    let drive = param_f64(&event.params, "drive")
        .unwrap_or(0.0)
        .clamp(0.0, 1.0) as f32;
    let cutoff = param_f64(&event.params, "cutoff_hz").unwrap_or(1200.0) as f32;
    let mut lowpass = OnePoleLowpass::new(cutoff, sample_rate as f32);
    let mut frames_out = Vec::with_capacity(frames);

    for frame in 0..frames {
        let t = frame as f32 / sample_rate as f32;
        let env = adsr(t as f64, note_seconds) as f32;
        let saw = 2.0 * ((freq * t) - (freq * t).floor()) - 1.0;
        let sub_freq = freq * 0.5;
        let sub = 2.0 * ((sub_freq * t) - (sub_freq * t).floor()) - 1.0;
        let mut value = (saw * 0.72 + sub * 0.28) * env;
        if drive > 0.0 {
            let amount = 1.0 + drive * 12.0;
            value = (value * amount).tanh() / amount.tanh();
        }
        value = lowpass.process(value) * gain;
        frames_out.push([value, value]);
    }

    mix_frames(buffer, start, &frames_out, 1.0, pan);
}

fn read_wav(path: &Path, expected_sample_rate: u32) -> Result<Vec<[f32; 2]>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open `{}`", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != expected_sample_rate {
        bail!(
            "sample `{}` has sample rate {}, expected {}",
            path.display(),
            spec.sample_rate,
            expected_sample_rate
        );
    }
    let channels = spec.channels as usize;
    if channels == 0 {
        bail!("sample `{}` has no channels", path.display());
    }

    let mono = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read `{}`", path.display()))?,
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                let scale = i16::MAX as f32;
                reader
                    .samples::<i16>()
                    .map(|sample| sample.map(|value| value as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()
                    .with_context(|| format!("failed to read `{}`", path.display()))?
            } else {
                let scale = ((1_i64 << (spec.bits_per_sample - 1)) - 1) as f32;
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| value as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()
                    .with_context(|| format!("failed to read `{}`", path.display()))?
            }
        }
    };

    let mut frames = Vec::with_capacity(mono.len() / channels);
    for chunk in mono.chunks(channels) {
        let left = chunk[0];
        let right = if channels > 1 { chunk[1] } else { left };
        frames.push([left, right]);
    }
    Ok(frames)
}

fn write_wav(path: &Path, sample_rate: u32, bit_depth: u16, buffer: &[[f32; 2]]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: bit_depth,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("failed to create `{}`", path.display()))?;
    match bit_depth {
        16 => {
            for frame in buffer {
                writer.write_sample(float_to_i16(frame[0]))?;
                writer.write_sample(float_to_i16(frame[1]))?;
            }
        }
        24 | 32 => {
            for frame in buffer {
                writer.write_sample(float_to_i32(frame[0], bit_depth))?;
                writer.write_sample(float_to_i32(frame[1], bit_depth))?;
            }
        }
        _ => unreachable!(),
    }
    writer.finalize()?;
    Ok(())
}

fn mix_frames(buffer: &mut [[f32; 2]], start: usize, source: &[[f32; 2]], gain: f32, pan: f32) {
    let pan = pan.clamp(-1.0, 1.0);
    let left_gain = ((1.0 - pan) * 0.5).sqrt() * gain;
    let right_gain = ((1.0 + pan) * 0.5).sqrt() * gain;
    for (index, frame) in source.iter().enumerate() {
        let Some(target) = buffer.get_mut(start + index) else {
            break;
        };
        target[0] += frame[0] * left_gain;
        target[1] += frame[1] * right_gain;
    }
}

fn adsr(t: f64, note_seconds: f64) -> f64 {
    let attack = 0.01;
    let decay = 0.18;
    let sustain = 0.65;
    let release = 0.08;
    if t < attack {
        t / attack
    } else if t < attack + decay {
        let progress = (t - attack) / decay;
        1.0 + (sustain - 1.0) * progress
    } else if t < note_seconds {
        sustain
    } else if t < note_seconds + release {
        let progress = (t - note_seconds) / release;
        sustain * (1.0 - progress)
    } else {
        0.0
    }
}

struct OnePoleLowpass {
    alpha: f32,
    state: f32,
}

impl OnePoleLowpass {
    fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.45);
        let rc = 1.0 / (2.0 * PI * cutoff);
        let dt = 1.0 / sample_rate;
        let alpha = dt / (rc + dt);
        Self { alpha, state: 0.0 }
    }

    fn process(&mut self, input: f32) -> f32 {
        self.state += self.alpha * (input - self.state);
        self.state
    }
}

fn param_f64(params: &BTreeMap<String, serde_json::Value>, name: &str) -> Option<f64> {
    params.get(name).and_then(|value| value.as_f64())
}

fn beats_to_seconds(beats: f64, tempo_bpm: f64) -> f64 {
    beats * 60.0 / tempo_bpm
}

fn seconds_to_frame(seconds: f64, sample_rate: u32) -> usize {
    (seconds * sample_rate as f64).round() as usize
}

fn db_to_amp(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

fn midi_to_freq(midi: i32) -> f64 {
    440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
}

fn sc_sample_format(bit_depth: u16) -> Result<&'static str> {
    match bit_depth {
        16 => Ok("int16"),
        24 => Ok("int24"),
        32 => Ok("int32"),
        _ => bail!("unsupported bit depth `{bit_depth}`; expected 16, 24, or 32"),
    }
}

fn sc_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn temp_script_path() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("malison-{nonce}.scd"))
}

fn float_to_i16(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn float_to_i32(value: f32, bit_depth: u16) -> i32 {
    let max = ((1_i64 << (bit_depth - 1)) - 1) as f32;
    (value.clamp(-1.0, 1.0) * max).round() as i32
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::compiler::{compile_events, project_root_for};
    use crate::parser::parse_source;

    #[test]
    fn renders_mvp_target_to_wav() {
        let root = std::env::temp_dir().join(format!("malison-render-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(
            root.join("malison.toml"),
            "[project]\nname = \"render-test\"\n",
        )
        .unwrap();
        write_test_kick(&root.join("samples/kick.wav"));

        let source = r#"
language 0.1

working "Render Test" {
  tempo 128
  meter 4/4
  seed "first"

  daemon kick = sample "samples/kick.wav" { gain -3 }
  daemon bass = saw_sub { cutoff 300 drive 0.3 }

  spell kicks = pattern "x---"
  spell bassline = notes "F1 -"

  rite main bars 1 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/render-test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let project_root = project_root_for(&path).unwrap();
        let compiled = compile_events(&path, &project_root, working).unwrap();
        let out = root.join("renders/render-test.wav");

        render_wav(&compiled, &out, 48_000, 24).unwrap();

        let reader = hound::WavReader::open(&out).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 48_000);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn generates_supercollider_nrt_script() {
        let root = std::env::temp_dir().join(format!("malison-sc-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("samples")).unwrap();
        fs::write(root.join("malison.toml"), "[project]\nname = \"sc-test\"\n").unwrap();
        write_test_kick(&root.join("samples/kick.wav"));

        let source = r#"
language 0.1

working "SC Test" {
  tempo 128
  meter 4/4
  seed "first"

  daemon kick = sample "samples/kick.wav" { gain -3 }
  daemon bass = saw_sub { cutoff 300 drive 0.3 }

  spell kicks = pattern "x---"
  spell bassline = notes "F1 -"

  rite main bars 1 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/sc-test.wav"
}
"#;
        let path = root.join("main.rite");
        fs::write(&path, source).unwrap();
        let working = parse_source(&path, source).unwrap();
        let project_root = project_root_for(&path).unwrap();
        let compiled = compile_events(&path, &project_root, working).unwrap();
        let script =
            supercollider_script(&compiled, &root.join("renders/sc-test.wav"), 48_000, 24).unwrap();

        assert!(script.contains("Score(["));
        assert!(script.contains("SynthDef(\\mal_sample"));
        assert!(script.contains("SynthDef(\\mal_saw_sub"));
        assert!(script.contains("\\b_allocRead"));
        assert!(script.contains("sampleFormat: \"int24\""));

        fs::remove_dir_all(&root).unwrap();
    }

    fn write_test_kick(path: &Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for index in 0..2400 {
            let t = index as f32 / 48_000.0;
            let env = (-40.0 * t).exp();
            let sample = (2.0 * PI * 90.0 * t).sin() * env;
            writer.write_sample(float_to_i16(sample)).unwrap();
        }
        writer.finalize().unwrap();
    }
}
