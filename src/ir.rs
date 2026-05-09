use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Ir {
    pub ir_version: String,
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
    pub semantic_path: String,
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
