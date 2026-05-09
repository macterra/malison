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
    pub source: IrSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrSpell {
    pub id: String,
    pub kind: String,
    pub body: String,
    pub source: IrSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrRite {
    pub id: String,
    pub start_beats: f64,
    pub duration_beats: f64,
    pub source: IrSource,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrEvent {
    pub id: String,
    pub semantic_path: String,
    pub kind: String,
    pub time_beats: f64,
    pub duration_beats: f64,
    pub daemon: String,
    pub velocity: f64,
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

#[derive(Clone, Debug, Serialize)]
pub struct IrGraph {
    pub ir_version: String,
    pub working: String,
    pub nodes: Vec<IrGraphNode>,
    pub edges: Vec<IrGraphEdge>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrGraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

impl Ir {
    pub fn graph(&self) -> IrGraph {
        let working_id = format!("working:{}", self.working);
        let mut nodes = vec![IrGraphNode {
            id: working_id.clone(),
            kind: "working".to_string(),
            label: self.working.clone(),
        }];
        let mut edges = Vec::new();

        for daemon in &self.daemons {
            let id = format!("daemon:{}", daemon.id);
            nodes.push(IrGraphNode {
                id: id.clone(),
                kind: "daemon".to_string(),
                label: daemon.id.clone(),
            });
            edges.push(IrGraphEdge {
                from: working_id.clone(),
                to: id,
                kind: "declares".to_string(),
            });
        }

        for spell in &self.spells {
            let id = format!("spell:{}", spell.id);
            nodes.push(IrGraphNode {
                id: id.clone(),
                kind: "spell".to_string(),
                label: spell.id.clone(),
            });
            edges.push(IrGraphEdge {
                from: working_id.clone(),
                to: id,
                kind: "declares".to_string(),
            });
        }

        for rite in &self.rites {
            let id = format!("rite:{}", rite.id);
            nodes.push(IrGraphNode {
                id: id.clone(),
                kind: "rite".to_string(),
                label: rite.id.clone(),
            });
            edges.push(IrGraphEdge {
                from: working_id.clone(),
                to: id,
                kind: "contains".to_string(),
            });
        }

        for event in &self.events {
            let id = format!("event:{}", event.id);
            nodes.push(IrGraphNode {
                id: id.clone(),
                kind: "event".to_string(),
                label: event.kind.clone(),
            });
            edges.push(IrGraphEdge {
                from: format!("daemon:{}", event.daemon),
                to: id.clone(),
                kind: "renders".to_string(),
            });
            if let Some(rite) = self.rites.iter().find(|rite| {
                event.time_beats >= rite.start_beats
                    && event.time_beats < rite.start_beats + rite.duration_beats
            }) {
                edges.push(IrGraphEdge {
                    from: format!("rite:{}", rite.id),
                    to: id,
                    kind: "contains".to_string(),
                });
            }
        }

        IrGraph {
            ir_version: self.ir_version.clone(),
            working: self.working.clone(),
            nodes,
            edges,
        }
    }
}
