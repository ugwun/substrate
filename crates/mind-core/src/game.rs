use crate::protocol::{OnboardingCompletionResult, OnboardingSnapshot};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io, io::Write, path::Path};
use tempfile::NamedTempFile;
use thiserror::Error;

const SAVE_SCHEMA_VERSION: u32 = 2;
const LEGACY_SAVE_SCHEMA_VERSION: u32 = 1;
pub const ONBOARDING_BAND_SLOTS: u8 = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameState {
    onboarding_complete: bool,
    state_revision: u64,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    generation: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveEnvelope {
    schema_version: u32,
    saved_at: DateTime<Utc>,
    state: GameState,
}

#[derive(Debug, Deserialize)]
struct VersionEnvelope {
    schema_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadDisposition {
    Missing,
    Current,
    LegacyReset,
}

#[derive(Debug)]
pub struct LoadedGame {
    pub state: GameState,
    pub disposition: LoadDisposition,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            state_revision: 0,
            dirty: true,
            generation: 0,
        }
    }
}

impl GameState {
    pub fn load(path: &Path) -> Result<LoadedGame, GameError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LoadedGame {
                    state: Self::default(),
                    disposition: LoadDisposition::Missing,
                });
            }
            Err(error) => return Err(GameError::Io(error)),
        };
        let value: Value = serde_json::from_slice(&bytes)?;
        let version: VersionEnvelope = serde_json::from_value(value.clone())?;
        if version.schema_version == LEGACY_SAVE_SCHEMA_VERSION {
            let mut state = Self::default();
            state.save(path)?;
            state.mark_saved(0);
            return Ok(LoadedGame {
                state,
                disposition: LoadDisposition::LegacyReset,
            });
        }
        if version.schema_version != SAVE_SCHEMA_VERSION {
            return Err(GameError::UnsupportedVersion(version.schema_version));
        }
        let envelope: SaveEnvelope = serde_json::from_value(value)?;
        let mut state = envelope.state;
        state.validate()?;
        state.dirty = false;
        state.generation = 0;
        Ok(LoadedGame {
            state,
            disposition: LoadDisposition::Current,
        })
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

    pub fn onboarding(&self) -> OnboardingSnapshot {
        OnboardingSnapshot {
            complete: self.onboarding_complete,
        }
    }

    pub fn complete_onboarding(&mut self, band_positions: [u8; 3]) -> OnboardingCompletionResult {
        if band_positions
            .iter()
            .any(|position| *position >= ONBOARDING_BAND_SLOTS)
        {
            return self.rejected("invalid_alignment");
        }
        if band_positions[1..]
            .iter()
            .any(|position| *position != band_positions[0])
        {
            return self.rejected("not_coherent");
        }
        if !self.onboarding_complete {
            self.onboarding_complete = true;
            self.state_revision += 1;
            self.dirty = true;
            self.generation += 1;
        }
        OnboardingCompletionResult {
            ok: true,
            code: None,
            state_revision: self.state_revision,
            completed: true,
        }
    }

    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub fn dirty_generation(&self) -> Option<u64> {
        self.dirty.then_some(self.generation)
    }

    pub fn mark_saved(&mut self, generation: u64) {
        if self.generation == generation {
            self.dirty = false;
        }
    }

    fn rejected(&self, code: &str) -> OnboardingCompletionResult {
        OnboardingCompletionResult {
            ok: false,
            code: Some(code.to_owned()),
            state_revision: self.state_revision,
            completed: self.onboarding_complete,
        }
    }

    fn validate(&self) -> Result<(), GameError> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum GameError {
    #[error("game-state I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid game-state JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("game-state path has no parent directory")]
    MissingParent,
    #[error("unsupported game state schema version {0}")]
    UnsupportedVersion(u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validates_alignment_and_completes_idempotently() {
        let mut state = GameState::default();
        assert_eq!(
            state.complete_onboarding([1, 2, 1]).code.as_deref(),
            Some("not_coherent")
        );
        assert_eq!(
            state.complete_onboarding([12, 12, 12]).code.as_deref(),
            Some("invalid_alignment")
        );
        assert_eq!(state.state_revision(), 0);

        let completed = state.complete_onboarding([7, 7, 7]);
        assert!(completed.ok);
        assert_eq!(completed.state_revision, 1);
        let repeated = state.complete_onboarding([3, 3, 3]);
        assert!(repeated.ok);
        assert_eq!(repeated.state_revision, 1);
    }

    #[test]
    fn persists_current_state_and_rejects_future_saves() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        let mut state = GameState::default();
        state.complete_onboarding([0, 0, 0]);
        state.save(&path).unwrap();

        let loaded = GameState::load(&path).unwrap();
        assert_eq!(loaded.disposition, LoadDisposition::Current);
        assert!(loaded.state.onboarding().complete);

        let future = fs::read_to_string(&path)
            .unwrap()
            .replace("\"schema_version\": 2", "\"schema_version\": 99");
        fs::write(&path, future).unwrap();
        assert!(matches!(
            GameState::load(&path),
            Err(GameError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn resets_legacy_save_without_deserializing_prototype_state() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("game_state.json");
        fs::write(
            &path,
            r#"{"schema_version":1,"saved_at":"2026-01-01T00:00:00Z","state":{"legacy":"discarded"}}"#,
        )
        .unwrap();

        let loaded = GameState::load(&path).unwrap();
        assert_eq!(loaded.disposition, LoadDisposition::LegacyReset);
        assert!(!loaded.state.onboarding().complete);
        let replacement: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(replacement["schema_version"], 2);
        assert_eq!(replacement["state"]["onboarding_complete"], false);
    }
}
