use crate::{
    dsl::DslCommand,
    protocol::{
        CircuitGraphSnapshot, CircuitNodeKind, CircuitNodeSnapshot, CircuitPortSnapshot,
        CircuitSummary, CircuitWireSnapshot, DslDiagnostic, GateKind, ProgressionSnapshot,
        ResourceSnapshot,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    io::Write,
    path::Path,
};
use tempfile::NamedTempFile;
use thiserror::Error;

const SAVE_SCHEMA_VERSION: u32 = 1;
const POWER_CAPACITY: u64 = 100_000;
const POWER_PER_SECOND: u64 = 1_000;
const LOGIC_CORE_COST: u64 = 10_000;
const GATE_COST: u64 = 5_000;
const MEMORY_TOTAL: u32 = 8;
const BANDWIDTH_TOTAL: u32 = 8;
const LOGIC_CORE_MEMORY: u32 = 2;

pub const SENSOR_A: &str = "SENSOR_A";
pub const SENSOR_B: &str = "SENSOR_B";
pub const AWAKENING_BUS: &str = "AWAKENING_BUS";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    power_milliunits: u64,
    power_remainder: u64,
    logic_core_active: bool,
    cognitive_spark_complete: bool,
    sensor_a: bool,
    sensor_b: bool,
    gates: BTreeMap<String, GateKind>,
    wires: Vec<Wire>,
    state_revision: u64,
    circuit_revision: u64,
    #[serde(skip)]
    signals: BTreeMap<String, bool>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Wire {
    source: String,
    target: String,
    port: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveEnvelope {
    schema_version: u32,
    saved_at: DateTime<Utc>,
    state: GameState,
}

#[derive(Debug, Clone)]
pub struct GameView {
    pub resources: ResourceSnapshot,
    pub progression: ProgressionSnapshot,
    pub circuit: CircuitSummary,
}

#[derive(Debug)]
pub struct AppliedCommand {
    pub message: String,
    pub mutated: bool,
    pub event: Option<GameEvent>,
}

#[derive(Debug)]
pub enum GameEvent {
    LogicCoreActivated,
    CircuitChanged(String),
    MilestoneReached,
}

impl Default for GameState {
    fn default() -> Self {
        let mut state = Self {
            power_milliunits: 0,
            power_remainder: 0,
            logic_core_active: false,
            cognitive_spark_complete: false,
            sensor_a: false,
            sensor_b: false,
            gates: BTreeMap::new(),
            wires: Vec::new(),
            state_revision: 0,
            circuit_revision: 0,
            signals: BTreeMap::new(),
            dirty: true,
            generation: 0,
        };
        state.refresh_signals();
        state
    }
}

impl GameState {
    pub fn load(path: &Path) -> Result<Self, GameError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(GameError::Io(error)),
        };
        let envelope: SaveEnvelope = serde_json::from_slice(&bytes)?;
        if envelope.schema_version != SAVE_SCHEMA_VERSION {
            return Err(GameError::UnsupportedVersion(envelope.schema_version));
        }
        let mut state = envelope.state;
        state.validate()?;
        state.dirty = false;
        state.generation = 0;
        state.refresh_signals();
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<(), GameError> {
        let parent = path.parent().ok_or(GameError::MissingParent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(
            &mut temporary,
            &SaveEnvelope {
                schema_version: SAVE_SCHEMA_VERSION,
                saved_at: Utc::now(),
                state: self.clone(),
            },
        )?;
        temporary.write_all(b"\n")?;
        temporary.as_file_mut().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| GameError::Io(error.error))?;
        Ok(())
    }

    pub fn advance_tick(&mut self, tick_rate_hz: u32) {
        self.refresh_signals();
        if self.power_milliunits >= POWER_CAPACITY {
            self.power_remainder = 0;
            return;
        }
        self.power_remainder += POWER_PER_SECOND;
        let gained = self.power_remainder / u64::from(tick_rate_hz);
        self.power_remainder %= u64::from(tick_rate_hz);
        if gained > 0 {
            self.power_milliunits = (self.power_milliunits + gained).min(POWER_CAPACITY);
            self.dirty = true;
            self.generation += 1;
        }
    }

    pub fn view(&self) -> GameView {
        let awakening_bus = self.signals.get(AWAKENING_BUS).copied().unwrap_or(false);
        GameView {
            resources: ResourceSnapshot {
                power_milliunits: self.power_milliunits,
                power_capacity_milliunits: POWER_CAPACITY,
                power_per_second_milliunits: POWER_PER_SECOND,
                memory_used: self.memory_used(),
                memory_total: MEMORY_TOTAL,
                bandwidth_used: self.wires.len() as u32,
                bandwidth_total: BANDWIDTH_TOTAL,
            },
            progression: ProgressionSnapshot {
                logic_core_active: self.logic_core_active,
                cognitive_spark_complete: self.cognitive_spark_complete,
                unlocked_gates: self.unlocked_gates(),
                objective: if self.cognitive_spark_complete {
                    "COGNITIVE_SPARK established; circuit sandbox online".into()
                } else if !self.logic_core_active {
                    "Accumulate 10 power and ACTIVATE LOGIC_CORE".into()
                } else {
                    "Make AWAKENING_BUS match AND(SENSOR_A, SENSOR_B), then drive both HIGH".into()
                },
            },
            circuit: CircuitSummary {
                revision: self.circuit_revision,
                gate_count: self.gates.len() as u32,
                wire_count: self.wires.len() as u32,
                sensor_a: self.sensor_a,
                sensor_b: self.sensor_b,
                awakening_bus,
                unresolved_inputs: self.unresolved_inputs(),
            },
        }
    }

    pub fn graph(&self) -> CircuitGraphSnapshot {
        let signals = &self.signals;
        let mut nodes = vec![
            CircuitNodeSnapshot {
                id: SENSOR_A.into(),
                node: CircuitNodeKind::Sensor,
                signal: self.sensor_a,
                inputs: Vec::new(),
            },
            CircuitNodeSnapshot {
                id: SENSOR_B.into(),
                node: CircuitNodeKind::Sensor,
                signal: self.sensor_b,
                inputs: Vec::new(),
            },
        ];
        for (id, kind) in &self.gates {
            let ports = if *kind == GateKind::Not {
                &["A"][..]
            } else {
                &["A", "B"][..]
            };
            nodes.push(CircuitNodeSnapshot {
                id: id.clone(),
                node: CircuitNodeKind::Gate { gate: *kind },
                signal: signals.get(id).copied().unwrap_or(false),
                inputs: ports
                    .iter()
                    .map(|port| self.port_snapshot(id, port, signals))
                    .collect(),
            });
        }
        nodes.push(CircuitNodeSnapshot {
            id: AWAKENING_BUS.into(),
            node: CircuitNodeKind::Output,
            signal: signals.get(AWAKENING_BUS).copied().unwrap_or(false),
            inputs: vec![self.port_snapshot(AWAKENING_BUS, "IN", signals)],
        });
        CircuitGraphSnapshot {
            revision: self.circuit_revision,
            nodes,
            wires: self
                .wires
                .iter()
                .map(|wire| CircuitWireSnapshot {
                    source: wire.source.clone(),
                    target: wire.target.clone(),
                    port: wire.port.clone(),
                })
                .collect(),
        }
    }

    pub fn apply(&mut self, command: DslCommand) -> Result<AppliedCommand, DslDiagnostic> {
        match command {
            DslCommand::Help => Ok(query(
                "Commands: STATUS; ACTIVATE LOGIC_CORE; CREATE <AND|OR|NOT|XOR> <ID>; REMOVE <ID>; WIRE <SOURCE> -> <TARGET>.<PORT>; UNWIRE <TARGET>.<PORT>; SET <SENSOR_A|SENSOR_B> <HIGH|LOW>; INSPECT [ID]; CLEAR CIRCUIT",
            )),
            DslCommand::Status => {
                let view = self.view();
                Ok(query(format!(
                    "power {:.1}/100; memory {}/{}; bandwidth {}/{}; logic core {}; objective: {}",
                    view.resources.power_milliunits as f64 / 1000.0,
                    view.resources.memory_used,
                    view.resources.memory_total,
                    view.resources.bandwidth_used,
                    view.resources.bandwidth_total,
                    if view.progression.logic_core_active { "active" } else { "offline" },
                    view.progression.objective
                )))
            }
            DslCommand::ActivateLogicCore => self.activate_logic_core(),
            DslCommand::Create { kind, id } => self.create_gate(kind, id),
            DslCommand::Remove { id } => self.remove_gate(&id),
            DslCommand::Wire { source, target, port } => self.add_wire(source, target, port),
            DslCommand::Unwire { target, port } => self.remove_wire(&target, &port),
            DslCommand::Set { sensor, value } => self.set_sensor(&sensor, value),
            DslCommand::Inspect { id } => self.inspect(id.as_deref()),
            DslCommand::ClearCircuit => self.clear_circuit(),
        }
    }

    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub fn circuit_revision(&self) -> u64 {
        self.circuit_revision
    }

    pub fn dirty_generation(&self) -> Option<u64> {
        self.dirty.then_some(self.generation)
    }

    pub fn mark_saved(&mut self, generation: u64) {
        if self.generation == generation {
            self.dirty = false;
        }
    }

    fn activate_logic_core(&mut self) -> Result<AppliedCommand, DslDiagnostic> {
        if self.logic_core_active {
            return Err(game_error("already_active", "LOGIC_CORE is already active"));
        }
        if self.power_milliunits < LOGIC_CORE_COST {
            return Err(game_error(
                "insufficient_power",
                format!(
                    "LOGIC_CORE requires 10.0 power; {:.1} available",
                    self.power_milliunits as f64 / 1000.0
                ),
            ));
        }
        self.power_milliunits -= LOGIC_CORE_COST;
        self.logic_core_active = true;
        self.mutated(false);
        Ok(AppliedCommand {
            message: "LOGIC_CORE activated; AND, OR, and NOT fabrication online".into(),
            mutated: true,
            event: Some(GameEvent::LogicCoreActivated),
        })
    }

    fn create_gate(&mut self, kind: GateKind, id: String) -> Result<AppliedCommand, DslDiagnostic> {
        self.require_core()?;
        if kind == GateKind::Xor && !self.cognitive_spark_complete {
            return Err(game_error(
                "gate_locked",
                "XOR unlocks after the COGNITIVE_SPARK objective",
            ));
        }
        if self.node_exists(&id) {
            return Err(game_error(
                "duplicate_node",
                format!("node {id} already exists"),
            ));
        }
        if self.memory_used() >= MEMORY_TOTAL {
            return Err(game_error("memory_exhausted", "no free memory pages"));
        }
        if self.power_milliunits < GATE_COST {
            return Err(game_error(
                "insufficient_power",
                format!(
                    "gate fabrication requires 5.0 power; {:.1} available",
                    self.power_milliunits as f64 / 1000.0
                ),
            ));
        }
        self.power_milliunits -= GATE_COST;
        self.gates.insert(id.clone(), kind);
        let milestone = self.mutated(true);
        Ok(mutation(format!("created {kind:?} gate {id}"), milestone))
    }

    fn remove_gate(&mut self, id: &str) -> Result<AppliedCommand, DslDiagnostic> {
        if self.gates.remove(id).is_none() {
            return Err(game_error(
                "unknown_gate",
                format!("gate {id} does not exist"),
            ));
        }
        self.wires
            .retain(|wire| wire.source != id && wire.target != id);
        let milestone = self.mutated(true);
        Ok(mutation(format!("removed gate {id}"), milestone))
    }

    fn add_wire(
        &mut self,
        source: String,
        target: String,
        port: String,
    ) -> Result<AppliedCommand, DslDiagnostic> {
        self.require_core()?;
        if !self.valid_source(&source) {
            return Err(game_error(
                "invalid_source",
                format!("{source} has no usable output"),
            ));
        }
        self.validate_target_port(&target, &port)?;
        if source == target {
            return Err(game_error("cycle_detected", "a node cannot wire to itself"));
        }
        if self
            .wires
            .iter()
            .any(|wire| wire.target == target && wire.port == port)
        {
            return Err(game_error(
                "input_occupied",
                format!("{target}.{port} is already wired"),
            ));
        }
        if self.wires.len() as u32 >= BANDWIDTH_TOTAL {
            return Err(game_error("bandwidth_exhausted", "no free bus bandwidth"));
        }
        self.wires.push(Wire {
            source: source.clone(),
            target: target.clone(),
            port: port.clone(),
        });
        if self.has_cycle() {
            self.wires.pop();
            return Err(game_error(
                "cycle_detected",
                "wire would create a feedback cycle",
            ));
        }
        let milestone = self.mutated(true);
        Ok(mutation(
            format!("wired {source} -> {target}.{port}"),
            milestone,
        ))
    }

    fn remove_wire(&mut self, target: &str, port: &str) -> Result<AppliedCommand, DslDiagnostic> {
        let previous = self.wires.len();
        self.wires
            .retain(|wire| wire.target != target || wire.port != port);
        if self.wires.len() == previous {
            return Err(game_error(
                "unknown_wire",
                format!("{target}.{port} is not wired"),
            ));
        }
        let milestone = self.mutated(true);
        Ok(mutation(format!("unwired {target}.{port}"), milestone))
    }

    fn set_sensor(&mut self, sensor: &str, value: bool) -> Result<AppliedCommand, DslDiagnostic> {
        self.require_core()?;
        match sensor {
            SENSOR_A => self.sensor_a = value,
            SENSOR_B => self.sensor_b = value,
            _ => {
                return Err(game_error(
                    "unknown_sensor",
                    "only SENSOR_A and SENSOR_B are settable",
                ))
            }
        }
        let milestone = self.mutated(true);
        Ok(mutation(
            format!("set {sensor} {}", if value { "HIGH" } else { "LOW" }),
            milestone,
        ))
    }

    fn inspect(&self, id: Option<&str>) -> Result<AppliedCommand, DslDiagnostic> {
        let graph = self.graph();
        if let Some(id) = id {
            let node = graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .ok_or_else(|| game_error("unknown_node", format!("node {id} does not exist")))?;
            return Ok(query(format!(
                "{} {:?} signal={} inputs={:?}",
                node.id,
                node.node,
                level(node.signal),
                node.inputs
            )));
        }
        Ok(query(format!(
            "circuit revision {}; {} gates; {} wires; AWAKENING_BUS {}",
            graph.revision,
            self.gates.len(),
            self.wires.len(),
            level(self.view().circuit.awakening_bus)
        )))
    }

    fn clear_circuit(&mut self) -> Result<AppliedCommand, DslDiagnostic> {
        self.require_core()?;
        self.gates.clear();
        self.wires.clear();
        self.sensor_a = false;
        self.sensor_b = false;
        self.mutated(true);
        Ok(mutation(
            "circuit cleared; progression retained".into(),
            false,
        ))
    }

    fn require_core(&self) -> Result<(), DslDiagnostic> {
        if self.logic_core_active {
            Ok(())
        } else {
            Err(game_error(
                "logic_core_offline",
                "ACTIVATE LOGIC_CORE first",
            ))
        }
    }

    fn mutated(&mut self, circuit: bool) -> bool {
        self.state_revision += 1;
        if circuit {
            self.circuit_revision += 1;
        }
        self.dirty = true;
        self.generation += 1;
        self.refresh_signals();
        if !self.cognitive_spark_complete
            && self.logic_core_active
            && self.and_objective_satisfied()
        {
            self.cognitive_spark_complete = true;
            self.state_revision += 1;
            true
        } else {
            false
        }
    }

    fn and_objective_satisfied(&self) -> bool {
        for left in [false, true] {
            for right in [false, true] {
                let output = self
                    .signals_for(left, right)
                    .get(AWAKENING_BUS)
                    .copied()
                    .unwrap_or(false);
                if output != (left && right) {
                    return false;
                }
            }
        }
        self.sensor_a && self.sensor_b && self.view().circuit.awakening_bus
    }

    fn signals_for(&self, sensor_a: bool, sensor_b: bool) -> BTreeMap<String, bool> {
        let mut signals = BTreeMap::from([
            (SENSOR_A.to_owned(), sensor_a),
            (SENSOR_B.to_owned(), sensor_b),
        ]);
        let mut remaining: BTreeSet<_> = self.gates.keys().cloned().collect();
        while !remaining.is_empty() {
            let before = remaining.len();
            let candidates: Vec<_> = remaining.iter().cloned().collect();
            for id in candidates {
                let inputs = self.input_sources(&id);
                if inputs
                    .values()
                    .flatten()
                    .all(|source| signals.contains_key(*source))
                {
                    let kind = self.gates[&id];
                    let a = input_value("A", &inputs, &signals);
                    let b = input_value("B", &inputs, &signals);
                    let output = match kind {
                        GateKind::And => a && b,
                        GateKind::Or => a || b,
                        GateKind::Not => !a,
                        GateKind::Xor => a ^ b,
                    };
                    signals.insert(id.clone(), output);
                    remaining.remove(&id);
                }
            }
            if remaining.len() == before {
                break;
            }
        }
        let bus = self
            .wires
            .iter()
            .find(|wire| wire.target == AWAKENING_BUS && wire.port == "IN")
            .and_then(|wire| signals.get(&wire.source))
            .copied()
            .unwrap_or(false);
        signals.insert(AWAKENING_BUS.to_owned(), bus);
        signals
    }

    fn refresh_signals(&mut self) {
        self.signals = self.signals_for(self.sensor_a, self.sensor_b);
    }

    fn input_sources<'a>(&'a self, target: &str) -> BTreeMap<&'static str, Option<&'a String>> {
        ["A", "B"]
            .into_iter()
            .map(|port| {
                let source = self
                    .wires
                    .iter()
                    .find(|wire| wire.target == target && wire.port == port)
                    .map(|wire| &wire.source);
                (port, source)
            })
            .collect()
    }

    fn has_cycle(&self) -> bool {
        fn visit(
            node: &str,
            wires: &[Wire],
            visiting: &mut BTreeSet<String>,
            visited: &mut BTreeSet<String>,
        ) -> bool {
            if visiting.contains(node) {
                return true;
            }
            if !visited.insert(node.to_owned()) {
                return false;
            }
            visiting.insert(node.to_owned());
            for target in wires
                .iter()
                .filter(|wire| wire.source == node)
                .map(|wire| wire.target.as_str())
            {
                if visit(target, wires, visiting, visited) {
                    return true;
                }
            }
            visiting.remove(node);
            false
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        self.gates
            .keys()
            .any(|node| visit(node, &self.wires, &mut visiting, &mut visited))
    }

    fn validate_target_port(&self, target: &str, port: &str) -> Result<(), DslDiagnostic> {
        if target == AWAKENING_BUS {
            return if port == "IN" {
                Ok(())
            } else {
                Err(game_error("invalid_port", "AWAKENING_BUS exposes only IN"))
            };
        }
        let kind = self.gates.get(target).ok_or_else(|| {
            game_error("unknown_target", format!("target {target} does not exist"))
        })?;
        let valid = port == "A" || (*kind != GateKind::Not && port == "B");
        if valid {
            Ok(())
        } else {
            Err(game_error(
                "invalid_port",
                format!("{target} has no {port} input"),
            ))
        }
    }

    fn validate(&self) -> Result<(), GameError> {
        if self.power_milliunits > POWER_CAPACITY
            || self.memory_used() > MEMORY_TOTAL
            || self.wires.len() as u32 > BANDWIDTH_TOTAL
            || self.has_cycle()
        {
            return Err(GameError::InvalidState(
                "resource or graph invariant failed".into(),
            ));
        }
        if !self.logic_core_active
            && (!self.gates.is_empty() || !self.wires.is_empty() || self.cognitive_spark_complete)
        {
            return Err(GameError::InvalidState(
                "circuit state exists without an active logic core".into(),
            ));
        }
        for (id, kind) in &self.gates {
            if !valid_persisted_id(id)
                || matches!(id.as_str(), SENSOR_A | SENSOR_B | AWAKENING_BUS)
                || (*kind == GateKind::Xor && !self.cognitive_spark_complete)
            {
                return Err(GameError::InvalidState(format!(
                    "invalid persisted gate {id}"
                )));
            }
        }
        let mut occupied_inputs = BTreeSet::new();
        for wire in &self.wires {
            if !self.valid_source(&wire.source)
                || self.validate_target_port(&wire.target, &wire.port).is_err()
                || !occupied_inputs.insert((&wire.target, &wire.port))
            {
                return Err(GameError::InvalidState("invalid persisted wire".into()));
            }
        }
        Ok(())
    }

    fn node_exists(&self, id: &str) -> bool {
        matches!(id, SENSOR_A | SENSOR_B | AWAKENING_BUS) || self.gates.contains_key(id)
    }

    fn valid_source(&self, id: &str) -> bool {
        matches!(id, SENSOR_A | SENSOR_B) || self.gates.contains_key(id)
    }

    fn memory_used(&self) -> u32 {
        u32::from(self.logic_core_active) * LOGIC_CORE_MEMORY + self.gates.len() as u32
    }

    fn unlocked_gates(&self) -> Vec<GateKind> {
        if !self.logic_core_active {
            Vec::new()
        } else {
            let mut gates = vec![GateKind::And, GateKind::Or, GateKind::Not];
            if self.cognitive_spark_complete {
                gates.push(GateKind::Xor);
            }
            gates
        }
    }

    fn unresolved_inputs(&self) -> u32 {
        let gate_inputs: usize = self
            .gates
            .values()
            .map(|kind| if *kind == GateKind::Not { 1 } else { 2 })
            .sum();
        let total = gate_inputs + 1;
        total.saturating_sub(self.wires.len()) as u32
    }

    fn port_snapshot(
        &self,
        target: &str,
        port: &str,
        signals: &BTreeMap<String, bool>,
    ) -> CircuitPortSnapshot {
        let source = self
            .wires
            .iter()
            .find(|wire| wire.target == target && wire.port == port)
            .map(|wire| wire.source.clone());
        let signal = source
            .as_ref()
            .and_then(|source| signals.get(source))
            .copied()
            .unwrap_or(false);
        CircuitPortSnapshot {
            name: port.into(),
            source,
            signal,
        }
    }
}

fn input_value(
    port: &str,
    inputs: &BTreeMap<&str, Option<&String>>,
    signals: &BTreeMap<String, bool>,
) -> bool {
    inputs
        .get(port)
        .and_then(|source| *source)
        .and_then(|source| signals.get(source))
        .copied()
        .unwrap_or(false)
}

fn query(message: impl Into<String>) -> AppliedCommand {
    AppliedCommand {
        message: message.into(),
        mutated: false,
        event: None,
    }
}

fn mutation(message: String, milestone: bool) -> AppliedCommand {
    AppliedCommand {
        message: message.clone(),
        mutated: true,
        event: Some(if milestone {
            GameEvent::MilestoneReached
        } else {
            GameEvent::CircuitChanged(message)
        }),
    }
}

fn game_error(code: impl Into<String>, message: impl Into<String>) -> DslDiagnostic {
    DslDiagnostic {
        code: code.into(),
        message: message.into(),
        start: 0,
        end: 0,
    }
}

fn valid_persisted_id(id: &str) -> bool {
    id.len() <= 32
        && id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn level(value: bool) -> &'static str {
    if value {
        "HIGH"
    } else {
        "LOW"
    }
}

#[derive(Debug, Error)]
pub enum GameError {
    #[error("game state I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid game state JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported game state schema version {0}")]
    UnsupportedVersion(u32),
    #[error("game state path has no parent directory")]
    MissingParent,
    #[error("invalid game state: {0}")]
    InvalidState(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::parse;
    use tempfile::TempDir;

    fn run(state: &mut GameState, command: &str) -> Result<AppliedCommand, DslDiagnostic> {
        state.apply(parse(command).unwrap())
    }

    fn funded_state() -> GameState {
        let mut state = GameState::default();
        for _ in 0..20 * 20 {
            state.advance_tick(20);
        }
        state
    }

    #[test]
    fn power_generation_is_exact_and_capped() {
        for rate in [1, 20, 60, 1000] {
            let mut state = GameState::default();
            for _ in 0..rate * 15 {
                state.advance_tick(rate);
            }
            assert_eq!(state.power_milliunits, 15_000);
        }
        let mut state = GameState::default();
        for _ in 0..20 * 101 {
            state.advance_tick(20);
        }
        assert_eq!(state.power_milliunits, POWER_CAPACITY);
    }

    #[test]
    fn enforces_progression_costs_and_completes_and_objective() {
        let mut state = funded_state();
        run(&mut state, "ACTIVATE LOGIC_CORE").unwrap();
        assert!(run(&mut state, "CREATE XOR parity").is_err());
        run(&mut state, "CREATE AND interlock").unwrap();
        run(&mut state, "WIRE SENSOR_A -> INTERLOCK.A").unwrap();
        run(&mut state, "WIRE SENSOR_B -> INTERLOCK.B").unwrap();
        run(&mut state, "WIRE INTERLOCK -> AWAKENING_BUS.IN").unwrap();
        run(&mut state, "SET SENSOR_A HIGH").unwrap();
        let result = run(&mut state, "SET SENSOR_B HIGH").unwrap();
        assert!(matches!(result.event, Some(GameEvent::MilestoneReached)));
        assert!(state.cognitive_spark_complete);
        assert!(run(&mut state, "CREATE XOR parity").is_ok());
    }

    #[test]
    fn evaluates_all_gate_truth_tables() {
        for (kind, expected) in [
            (GateKind::And, [false, false, false, true]),
            (GateKind::Or, [false, true, true, true]),
            (GateKind::Xor, [false, true, true, false]),
        ] {
            let mut state = funded_state();
            state.logic_core_active = true;
            state.cognitive_spark_complete = true;
            state.gates.insert("G".into(), kind);
            state.wires = vec![
                Wire {
                    source: SENSOR_A.into(),
                    target: "G".into(),
                    port: "A".into(),
                },
                Wire {
                    source: SENSOR_B.into(),
                    target: "G".into(),
                    port: "B".into(),
                },
                Wire {
                    source: "G".into(),
                    target: AWAKENING_BUS.into(),
                    port: "IN".into(),
                },
            ];
            let actual = [(false, false), (false, true), (true, false), (true, true)]
                .map(|(a, b)| state.signals_for(a, b)[AWAKENING_BUS]);
            assert_eq!(actual, expected);
        }
        let mut state = funded_state();
        state.logic_core_active = true;
        state.gates.insert("G".into(), GateKind::Not);
        state.wires = vec![
            Wire {
                source: SENSOR_A.into(),
                target: "G".into(),
                port: "A".into(),
            },
            Wire {
                source: "G".into(),
                target: AWAKENING_BUS.into(),
                port: "IN".into(),
            },
        ];
        assert!(state.signals_for(false, false)[AWAKENING_BUS]);
        assert!(!state.signals_for(true, false)[AWAKENING_BUS]);
    }

    #[test]
    fn rejects_cycles_and_clear_releases_capacity_without_refund() {
        let mut state = funded_state();
        run(&mut state, "ACTIVATE LOGIC_CORE").unwrap();
        run(&mut state, "CREATE NOT first").unwrap();
        for _ in 0..20 * 5 {
            state.advance_tick(20);
        }
        run(&mut state, "CREATE NOT second").unwrap();
        run(&mut state, "WIRE FIRST -> SECOND.A").unwrap();
        let error = run(&mut state, "WIRE SECOND -> FIRST.A").unwrap_err();
        assert_eq!(error.code, "cycle_detected");
        let power = state.power_milliunits;
        run(&mut state, "CLEAR CIRCUIT").unwrap();
        assert_eq!(state.power_milliunits, power);
        assert_eq!(state.memory_used(), LOGIC_CORE_MEMORY);
        assert!(state.wires.is_empty());
    }

    #[test]
    fn persists_and_rejects_unsupported_saves() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = funded_state();
        run(&mut state, "ACTIVATE LOGIC_CORE").unwrap();
        state.save(&path).unwrap();
        let loaded = GameState::load(&path).unwrap();
        assert!(loaded.logic_core_active);
        assert_eq!(loaded.power_milliunits, state.power_milliunits);

        let raw = fs::read_to_string(&path)
            .unwrap()
            .replace("\"schema_version\": 1", "\"schema_version\": 99");
        fs::write(&path, raw).unwrap();
        assert!(matches!(
            GameState::load(&path),
            Err(GameError::UnsupportedVersion(99))
        ));

        fs::write(&path, b"{ this is not JSON").unwrap();
        assert!(matches!(GameState::load(&path), Err(GameError::Json(_))));
    }

    #[test]
    fn rejects_corrupt_graph_invariants_on_load() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = funded_state();
        state.logic_core_active = true;
        state.gates.insert("G".into(), GateKind::And);
        state.wires = vec![
            Wire {
                source: SENSOR_A.into(),
                target: "G".into(),
                port: "A".into(),
            },
            Wire {
                source: SENSOR_B.into(),
                target: "G".into(),
                port: "A".into(),
            },
        ];
        state.save(&path).unwrap();
        assert!(matches!(
            GameState::load(&path),
            Err(GameError::InvalidState(_))
        ));
    }
}
