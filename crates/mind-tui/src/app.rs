use mind_core::protocol::{
    CircuitGraphSnapshot, DiagnosticPulseResult, DslExecutionResult, EngineEvent, StatusSnapshot,
};
use std::{collections::VecDeque, time::Instant};

const EVENT_CACHE_CAPACITY: usize = 256;
const COMMAND_HISTORY_CAPACITY: usize = 100;
const TRANSCRIPT_CAPACITY: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Dashboard,
    Assembly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptEntry {
    pub command: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Debug)]
pub struct App {
    pub connection: ConnectionState,
    pub snapshot: Option<StatusSnapshot>,
    pub events: VecDeque<EngineEvent>,
    pub observed_tick_rate: Option<f64>,
    pub last_update: Option<Instant>,
    pub event_scroll: usize,
    pub follow_events: bool,
    pub show_help: bool,
    pub notice: Option<String>,
    pub workspace: Workspace,
    pub graph: Option<CircuitGraphSnapshot>,
    pub prompt_focused: bool,
    pub prompt: String,
    pub prompt_cursor: usize,
    pub command_history: VecDeque<String>,
    pub transcript: VecDeque<TranscriptEntry>,
    history_position: Option<usize>,
    previous_tick_sample: Option<(u64, Instant)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Connecting,
            snapshot: None,
            events: VecDeque::new(),
            observed_tick_rate: None,
            last_update: None,
            event_scroll: 0,
            follow_events: true,
            show_help: false,
            notice: None,
            workspace: Workspace::Dashboard,
            graph: None,
            prompt_focused: false,
            prompt: String::new(),
            prompt_cursor: 0,
            command_history: VecDeque::new(),
            transcript: VecDeque::new(),
            history_position: None,
            previous_tick_sample: None,
        }
    }
}

impl App {
    pub fn connected(&mut self) {
        self.connection = ConnectionState::Connected;
        self.notice = None;
    }

    pub fn connecting(&mut self) {
        self.connection = ConnectionState::Connecting;
    }

    pub fn disconnected(&mut self, message: impl Into<String>) {
        self.connection = ConnectionState::Disconnected(message.into());
        self.previous_tick_sample = None;
        self.observed_tick_rate = None;
    }

    pub fn apply_snapshot(&mut self, snapshot: StatusSnapshot, received_at: Instant) {
        if let Some((previous_tick, previous_at)) = self.previous_tick_sample {
            let elapsed = received_at
                .saturating_duration_since(previous_at)
                .as_secs_f64();
            if elapsed > 0.0 && snapshot.tick >= previous_tick {
                self.observed_tick_rate = Some((snapshot.tick - previous_tick) as f64 / elapsed);
            }
        }
        self.previous_tick_sample = Some((snapshot.tick, received_at));

        let mut inserted = 0;
        for event in &snapshot.recent_events {
            if self
                .events
                .iter()
                .all(|known| known.event_id != event.event_id)
            {
                self.events.push_back(event.clone());
                inserted += 1;
            }
        }
        while self.events.len() > EVENT_CACHE_CAPACITY {
            self.events.pop_front();
        }
        if self.follow_events {
            self.event_scroll = 0;
        } else {
            self.event_scroll = self.event_scroll.saturating_add(inserted);
            self.clamp_scroll();
        }

        self.last_update = Some(received_at);
        self.snapshot = Some(snapshot);
        self.connection = ConnectionState::Connected;
    }

    pub fn diagnostic_completed(&mut self, result: &DiagnosticPulseResult) {
        self.notice = Some(format!("DIAGNOSTIC PULSE {} COMPLETE", result.pulse_id));
    }

    pub fn diagnostic_failed(&mut self, error: impl Into<String>) {
        self.notice = Some(format!("DIAGNOSTIC FAILED: {}", error.into()));
    }

    pub fn apply_graph(&mut self, graph: CircuitGraphSnapshot) {
        self.graph = Some(graph);
    }

    pub fn apply_dsl_result(&mut self, result: DslExecutionResult) {
        self.transcript.push_back(TranscriptEntry {
            command: result.command,
            ok: result.ok,
            message: result.message,
        });
        while self.transcript.len() > TRANSCRIPT_CAPACITY {
            self.transcript.pop_front();
        }
    }

    pub fn set_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
        self.prompt_focused = workspace == Workspace::Assembly;
        self.show_help = false;
    }

    pub fn focus_prompt(&mut self) {
        self.prompt_focused = true;
        self.prompt_cursor = self.prompt.len();
    }

    pub fn blur_prompt(&mut self) {
        self.prompt_focused = false;
        self.history_position = None;
    }

    pub fn insert_prompt_char(&mut self, character: char) {
        self.prompt.insert(self.prompt_cursor, character);
        self.prompt_cursor += character.len_utf8();
    }

    pub fn backspace_prompt(&mut self) {
        if self.prompt_cursor == 0 {
            return;
        }
        let previous = self.prompt[..self.prompt_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
        self.prompt.drain(previous..self.prompt_cursor);
        self.prompt_cursor = previous;
    }

    pub fn delete_prompt(&mut self) {
        if self.prompt_cursor >= self.prompt.len() {
            return;
        }
        let next = self.prompt[self.prompt_cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.prompt.len(), |(index, _)| self.prompt_cursor + index);
        self.prompt.drain(self.prompt_cursor..next);
    }

    pub fn move_prompt_left(&mut self) {
        self.prompt_cursor = self.prompt[..self.prompt_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }

    pub fn move_prompt_right(&mut self) {
        if self.prompt_cursor < self.prompt.len() {
            self.prompt_cursor = self.prompt[self.prompt_cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.prompt.len(), |(index, _)| self.prompt_cursor + index);
        }
    }

    pub fn take_prompt_command(&mut self) -> Option<String> {
        let command = self.prompt.trim().to_owned();
        if command.is_empty() {
            return None;
        }
        if self.command_history.back() != Some(&command) {
            self.command_history.push_back(command.clone());
            while self.command_history.len() > COMMAND_HISTORY_CAPACITY {
                self.command_history.pop_front();
            }
        }
        self.prompt.clear();
        self.prompt_cursor = 0;
        self.history_position = None;
        Some(command)
    }

    pub fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let position = self
            .history_position
            .map_or(self.command_history.len() - 1, |position| {
                position.saturating_sub(1)
            });
        self.history_position = Some(position);
        self.prompt = self.command_history[position].clone();
        self.prompt_cursor = self.prompt.len();
    }

    pub fn history_next(&mut self) {
        let Some(position) = self.history_position else {
            return;
        };
        if position + 1 < self.command_history.len() {
            self.history_position = Some(position + 1);
            self.prompt = self.command_history[position + 1].clone();
        } else {
            self.history_position = None;
            self.prompt.clear();
        }
        self.prompt_cursor = self.prompt.len();
    }

    pub fn complete_prompt(&mut self) {
        let before = &self.prompt[..self.prompt_cursor];
        let token_start = before
            .rfind(char::is_whitespace)
            .map_or(0, |position| position + 1);
        let prefix = before[token_start..].to_ascii_uppercase();
        let upper = before.to_ascii_uppercase();
        let mut candidates: Vec<String> = if upper.starts_with("CREATE ") {
            ["AND", "OR", "NOT", "XOR"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else if upper.starts_with("SET ") && upper.split_whitespace().count() <= 2 {
            vec!["SENSOR_A".into(), "SENSOR_B".into()]
        } else if upper.starts_with("SET ") {
            vec!["HIGH".into(), "LOW".into()]
        } else if upper.starts_with("WIRE ") && !upper.contains("->") {
            self.source_ids()
        } else if upper.contains("->") {
            self.target_ids()
        } else {
            [
                "HELP", "STATUS", "ACTIVATE", "CREATE", "REMOVE", "WIRE", "UNWIRE", "SET",
                "INSPECT", "CLEAR",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        };
        candidates.sort();
        if let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| candidate.starts_with(&prefix) && candidate != &prefix)
        {
            self.prompt
                .replace_range(token_start..self.prompt_cursor, &candidate);
            self.prompt_cursor = token_start + candidate.len();
        }
    }

    fn source_ids(&self) -> Vec<String> {
        self.graph.as_ref().map_or_else(
            || vec!["SENSOR_A".into(), "SENSOR_B".into()],
            |graph| {
                graph
                    .nodes
                    .iter()
                    .filter(|node| node.id != "AWAKENING_BUS")
                    .map(|node| node.id.clone())
                    .collect()
            },
        )
    }

    fn target_ids(&self) -> Vec<String> {
        self.graph.as_ref().map_or_else(
            || vec!["AWAKENING_BUS.IN".into()],
            |graph| {
                graph
                    .nodes
                    .iter()
                    .filter(|node| !node.inputs.is_empty())
                    .flat_map(|node| {
                        node.inputs
                            .iter()
                            .map(|port| format!("{}.{}", node.id, port.name))
                    })
                    .collect()
            },
        )
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.follow_events = false;
        self.event_scroll = self.event_scroll.saturating_add(amount);
        self.clamp_scroll();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.event_scroll = self.event_scroll.saturating_sub(amount);
        if self.event_scroll == 0 {
            self.follow_events = true;
        }
    }

    pub fn scroll_home(&mut self) {
        self.follow_events = false;
        self.event_scroll = self.events.len().saturating_sub(1);
    }

    pub fn scroll_end(&mut self) {
        self.follow_events = true;
        self.event_scroll = 0;
    }

    pub fn toggle_follow(&mut self) {
        self.follow_events = !self.follow_events;
        if self.follow_events {
            self.event_scroll = 0;
        }
    }

    fn clamp_scroll(&mut self) {
        self.event_scroll = self.event_scroll.min(self.events.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mind_core::protocol::{
        CircuitSummary, EngineEventKind, HealthState, ProgressionSnapshot, ResourceSnapshot,
    };
    use std::time::Duration;

    fn snapshot(tick: u64, events: Vec<EngineEvent>) -> StatusSnapshot {
        StatusSnapshot {
            protocol_version: 2,
            daemon_version: "0.1.0".into(),
            pid: 42,
            health: HealthState::Ready,
            started_at: Utc::now(),
            uptime_ms: tick * 50,
            tick,
            tick_rate_hz: 20,
            cpu_usage_percent: Some(2.5),
            resources: ResourceSnapshot {
                power_milliunits: 0,
                power_capacity_milliunits: 100_000,
                power_per_second_milliunits: 1_000,
                memory_used: 0,
                memory_total: 8,
                bandwidth_used: 0,
                bandwidth_total: 8,
            },
            progression: ProgressionSnapshot {
                logic_core_active: false,
                cognitive_spark_complete: false,
                unlocked_gates: vec![],
                objective: "Activate core".into(),
            },
            circuit: CircuitSummary {
                revision: 0,
                gate_count: 0,
                wire_count: 0,
                sensor_a: false,
                sensor_b: false,
                awakening_bus: false,
                unresolved_inputs: 1,
            },
            connected_clients: 1,
            mcp_url: "http://127.0.0.1:7777/mcp".into(),
            last_diagnostic: None,
            recent_events: events,
        }
    }

    fn event(id: u64) -> EngineEvent {
        EngineEvent {
            event_id: id,
            occurred_at: Utc::now(),
            tick: id,
            kind: EngineEventKind::DiagnosticPulse,
            message: format!("event {id}"),
        }
    }

    #[test]
    fn derives_observed_rate_and_deduplicates_events() {
        let mut app = App::default();
        let start = Instant::now();
        app.apply_snapshot(snapshot(10, vec![event(1)]), start);
        app.apply_snapshot(
            snapshot(30, vec![event(1), event(2)]),
            start + Duration::from_secs(1),
        );
        assert_eq!(app.observed_tick_rate, Some(20.0));
        assert_eq!(app.events.len(), 2);
    }

    #[test]
    fn preserves_scrolled_content_when_new_events_arrive() {
        let mut app = App::default();
        let start = Instant::now();
        app.apply_snapshot(snapshot(1, vec![event(1), event(2)]), start);
        app.scroll_up(1);
        app.apply_snapshot(
            snapshot(2, vec![event(2), event(3)]),
            start + Duration::from_secs(1),
        );
        assert!(!app.follow_events);
        assert_eq!(app.event_scroll, 2);
        app.scroll_end();
        assert!(app.follow_events);
        assert_eq!(app.event_scroll, 0);
    }

    #[test]
    fn edits_completes_and_recalls_dsl_commands() {
        let mut app = App::default();
        app.set_workspace(Workspace::Assembly);
        for character in "sta".chars() {
            app.insert_prompt_char(character);
        }
        app.complete_prompt();
        assert_eq!(app.prompt, "STATUS");
        assert_eq!(app.take_prompt_command().as_deref(), Some("STATUS"));
        app.history_previous();
        assert_eq!(app.prompt, "STATUS");
        app.backspace_prompt();
        assert_eq!(app.prompt, "STATU");
    }
}
