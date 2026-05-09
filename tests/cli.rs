use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn check_accepts_valid_rite_project() {
    let fixture = Fixture::new();

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn events_outputs_deterministic_json() {
    let fixture = Fixture::new();

    let output = Command::cargo_bin("malison")
        .unwrap()
        .arg("events")
        .arg(fixture.main_rite())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    normalize_source_files(&mut json);
    assert_eq!(json["language"], "0.1");
    assert_eq!(json["working"], "CLI Test");
    assert_eq!(json["events"].as_array().unwrap().len(), 8);
    assert_eq!(json["events"][0]["kind"], "trigger");
    assert_eq!(json["events"][1]["kind"], "note");
    insta::assert_json_snapshot!("events_cli_test", json);
}

#[test]
fn ir_outputs_deterministic_json() {
    let fixture = Fixture::new();

    let output = Command::cargo_bin("malison")
        .unwrap()
        .arg("ir")
        .arg(fixture.main_rite())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    normalize_source_files(&mut json);
    assert_eq!(json["language"], "0.1");
    assert_eq!(json["working"], "CLI Test");
    assert_eq!(json["daemons"].as_array().unwrap().len(), 2);
    assert_eq!(json["spells"].as_array().unwrap().len(), 2);
    assert_eq!(json["rites"].as_array().unwrap().len(), 1);
    insta::assert_json_snapshot!("ir_cli_test", json);
}

#[test]
fn render_rust_backend_writes_wav() {
    let fixture = Fixture::new();
    let out = fixture.root.path().join("renders/cli-test.wav");

    Command::cargo_bin("malison")
        .unwrap()
        .arg("render")
        .arg(fixture.main_rite())
        .arg("--force")
        .assert()
        .success();

    assert!(out.exists());
    let reader = hound::WavReader::open(out).unwrap();
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().sample_rate, 48_000);
}

#[test]
fn render_supercollider_dry_run_outputs_score() {
    let fixture = Fixture::new();

    let output = Command::cargo_bin("malison")
        .unwrap()
        .arg("render")
        .arg(fixture.main_rite())
        .arg("--backend")
        .arg("supercollider")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Score(["))
        .stdout(predicate::str::contains("SynthDef(\\mal_saw_sub"))
        .get_output()
        .stdout
        .clone();

    let script = String::from_utf8(output).unwrap();
    insta::assert_snapshot!(
        "supercollider_dry_run",
        normalize_supercollider_script(&script, fixture.root.path())
    );
}

#[test]
fn scry_outputs_human_readable_summary() {
    let fixture = Fixture::new();

    Command::cargo_bin("malison")
        .unwrap()
        .arg("scry")
        .arg(fixture.main_rite())
        .assert()
        .success()
        .stdout(predicate::str::contains("working: CLI Test"))
        .stdout(predicate::str::contains("events: 8"))
        .stdout(predicate::str::contains("rite main"))
        .stdout(predicate::str::contains("note    bass"));
}

#[test]
fn version_flag_reports_package_version() {
    Command::cargo_bin("malison")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn rejects_non_rite_source_extension() {
    let fixture = Fixture::new();
    let bad = fixture.root.path().join("main.txt");
    fs::copy(fixture.main_rite(), &bad).unwrap();

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(bad)
        .assert()
        .failure()
        .stderr(predicate::str::contains("must use the .rite extension"));
}

#[test]
fn rejects_out_of_range_pan() {
    let fixture = Fixture::new_with_source(&RITE.replace("gain -3", "gain -3 pan 2"));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "parameter `pan` must be in [-1, 1]",
        ));
}

#[test]
fn rejects_non_numeric_drive() {
    let fixture = Fixture::new_with_source(&RITE.replace("drive 0.3", "drive hot"));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "parameter `drive` must be numeric",
        ));
}

#[test]
fn rejects_zero_every_duration() {
    let fixture = Fixture::new_with_source(&RITE.replace("every 1/16", "every 0"));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`every` duration must be positive",
        ));
}

#[test]
fn rejects_empty_rite() {
    let fixture = Fixture::new_with_source(&RITE.replace(
        "    invoke kick with kicks every 1/16\n    invoke bass with bassline every 1/8\n",
        "",
    ));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains("must contain at least one invoke"));
}

#[test]
fn rejects_nonpositive_tempo() {
    let fixture = Fixture::new_with_source(&RITE.replace("tempo 128", "tempo 0"));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains("tempo must be positive"));
}

#[test]
fn rejects_unsupported_meter_denominator() {
    let fixture = Fixture::new_with_source(&RITE.replace("meter 4/4", "meter 4/7"));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported meter denominator `7`",
        ));
}

#[test]
fn rejects_daemon_spell_type_mismatch() {
    let fixture = Fixture::new_with_source(&RITE.replace(
        "invoke kick with kicks every 1/16",
        "invoke kick with bassline every 1/16",
    ));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "daemon `kick` cannot be invoked with spell `bassline`",
        ));
}

#[test]
fn suggests_nearby_daemon_name() {
    let fixture = Fixture::new_with_source(&RITE.replace(
        "invoke bass with bassline every 1/8",
        "invoke basss with bassline every 1/8",
    ));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unresolved daemon `basss`"))
        .stderr(predicate::str::contains("did you mean `bass`?"));
}

#[test]
fn suggests_nearby_spell_name() {
    let fixture = Fixture::new_with_source(&RITE.replace(
        "invoke bass with bassline every 1/8",
        "invoke bass with basslin every 1/8",
    ));

    Command::cargo_bin("malison")
        .unwrap()
        .arg("check")
        .arg(fixture.main_rite())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unresolved spell `basslin`"))
        .stderr(predicate::str::contains("did you mean `bassline`?"));
}

#[test]
fn rejects_output_parent_that_is_file() {
    let fixture = Fixture::new();
    let bad_parent = fixture.root.path().join("not-a-dir");
    fs::write(&bad_parent, "nope").unwrap();

    Command::cargo_bin("malison")
        .unwrap()
        .arg("render")
        .arg(fixture.main_rite())
        .arg("--out")
        .arg(bad_parent.join("out.wav"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a directory"));
}

struct Fixture {
    root: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self::new_with_source(RITE)
    }

    fn new_with_source(source: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("samples")).unwrap();
        fs::write(
            root.path().join("malison.toml"),
            "[project]\nname = \"cli-test\"\n",
        )
        .unwrap();
        write_test_kick(&root.path().join("samples/kick.wav"));
        fs::write(root.path().join("main.rite"), source).unwrap();
        Self { root }
    }

    fn main_rite(&self) -> std::path::PathBuf {
        self.root.path().join("main.rite")
    }
}

const RITE: &str = r#"
language 0.1

working "CLI Test" {
  tempo 128
  meter 4/4
  seed "cli"

  daemon kick = sample "samples/kick.wav" { gain -3 }
  daemon bass = saw_sub { cutoff 300 drive 0.3 }

  spell kicks = pattern "x---"
  spell bassline = notes "F1 -"

  rite main bars 1 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/cli-test.wav"
}
"#;

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
        let sample = (2.0 * std::f32::consts::PI * 90.0 * t).sin() * env;
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            .unwrap();
    }
    writer.finalize().unwrap();
}

fn normalize_source_files(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(source) = object.get_mut("source")
                && let Some(source_object) = source.as_object_mut()
                && source_object.contains_key("file")
            {
                source_object.insert(
                    "file".to_string(),
                    serde_json::Value::String("<fixture>/main.rite".to_string()),
                );
            }
            for child in object.values_mut() {
                normalize_source_files(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                normalize_source_files(child);
            }
        }
        _ => {}
    }
}

fn normalize_supercollider_script(script: &str, root: &Path) -> String {
    script.replace(&root.display().to_string(), "<fixture>")
}
