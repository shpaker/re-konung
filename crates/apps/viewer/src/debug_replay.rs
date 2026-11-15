//! Headless replay of the same runtime commands used by the live viewer.
use crate::cli::{Cli, Format};
use kn_game::{
    Game, MapRef,
    debug::{DebugJournal, DebugScript, DebugSnapshot},
};
use serde::Serialize;
use std::{path::Path, process::ExitCode};

#[derive(Debug, Serialize)]
pub struct ReplayFrame {
    pub tick: u64,
    pub sequence: Option<u64>,
    pub result: Option<Result<(), String>>,
    pub snapshot: DebugSnapshot,
}
#[derive(Debug, Serialize)]
pub struct DebugReplayReport {
    pub version: u32,
    pub map: MapRef,
    pub frames: Vec<ReplayFrame>,
    pub final_snapshot: DebugSnapshot,
    pub script: DebugScript,
}

pub fn replay(game: &Game, script: &DebugScript) -> Result<DebugReplayReport, String> {
    script.validate()?;
    let profile = game.debug_player_profile(script.map, script.hero).ok();
    script.validate_profile(profile.as_ref())?;
    let fresh = || {
        game.simulation_with_hero(script.map, script.seed, script.hero)
            .map_err(|error| error.to_string())
    };
    let mut sim = fresh()?;
    let mut journal = DebugJournal::default();
    journal.replace_entries(script.entries.clone())?;
    let mut frames = Vec::new();
    loop {
        let applied = journal.applied_count();
        // A single work unit yields exactly at a command boundary, including
        // multiple commands at one tick; there is no second command executor.
        let progress = journal.seek(&mut sim, profile.as_ref(), script.through_tick, 1, fresh)?;
        if journal.applied_count() != applied {
            let entry = &journal.entries()[journal.applied_count() - 1];
            frames.push(ReplayFrame {
                tick: entry.tick,
                sequence: Some(entry.sequence),
                result: entry.result.clone(),
                snapshot: sim.debug_snapshot(),
            });
        }
        if progress.caught_up {
            break;
        }
    }
    let final_snapshot = sim.debug_snapshot();
    frames.push(ReplayFrame {
        tick: final_snapshot.tick,
        sequence: None,
        result: None,
        snapshot: final_snapshot.clone(),
    });
    let mut resolved = script.clone();
    resolved.profile = profile;
    resolved.entries = journal.entries().to_vec();
    Ok(DebugReplayReport {
        version: 1,
        map: script.map,
        frames,
        final_snapshot,
        script: resolved,
    })
}

pub fn load_script(path: &Path) -> Result<DebugScript, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let script: DebugScript =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid debug script: {error}"))?;
    script.validate()?;
    Ok(script)
}

pub fn run(cli: &Cli, path: &Path, format: Format) -> ExitCode {
    let result = (|| -> Result<(), String> {
        let script = load_script(path)?;
        let root = cli
            .game_dir
            .as_ref()
            .ok_or("--game-dir is required for debug-replay")?;
        let game = Game::open(root).map_err(|error| error.to_string())?;
        let report = replay(&game, &script)?;
        match format {
            Format::Json => println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            ),
            Format::Text => {
                println!(
                    "{} {} · logical tick {} · {} command boundaries",
                    report.map.source,
                    report.map.number,
                    report.final_snapshot.tick,
                    report.frames.len() - 1
                );
                for frame in &report.frames {
                    if let Some(sequence) = frame.sequence {
                        println!(
                            "tick {} · command {} · {:?}",
                            frame.tick, sequence, frame.result
                        );
                    }
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("debug-replay: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kn_game::debug::{DebugCommand, DebugPlayerProfile, JournalEntry};
    fn script() -> DebugScript {
        DebugScript {
            version: 1,
            map: MapRef {
                source: kn_game::MapSource::Game,
                number: 17,
            },
            hero: None,
            seed: 42,
            profile: None,
            entries: vec![JournalEntry {
                tick: 0,
                sequence: 0,
                command: DebugCommand::Remove,
                result: None,
            }],
            through_tick: 0,
        }
    }
    #[test]
    fn authored_scripts_need_no_predicted_result_or_profile() {
        let parsed:DebugScript=serde_json::from_str(r#"{"version":1,"map":{"source":"game","number":17},"hero":null,"seed":42,"entries":[{"tick":0,"sequence":0,"command":{"type":"remove"}}],"through_tick":0}"#).unwrap();
        parsed.validate().unwrap();
        assert!(parsed.entries[0].result.is_none());
    }
    #[test]
    fn invalid_schema_order_and_supplied_profile_are_rejected_before_replay() {
        let mut value = serde_json::to_value(script()).unwrap();
        value["extra"] = true.into();
        assert!(serde_json::from_value::<DebugScript>(value).is_err());
        let mut entry_typo = serde_json::to_value(script()).unwrap();
        entry_typo["entries"][0]["sequnce"] = 1.into();
        assert!(serde_json::from_value::<DebugScript>(entry_typo).is_err());
        let mut invalid = script();
        invalid.version = 2;
        assert!(invalid.validate().is_err());
        let mut invalid = script();
        invalid.entries.push(invalid.entries[0].clone());
        assert!(invalid.validate().is_err());
        let profile = DebugPlayerProfile {
            hero: 0,
            character: 0,
            group: 0,
            direction: 0,
            class_id: 1,
            recorded_state: 0,
            step_ticks: [7; 8],
        };
        let mut invalid = script();
        invalid.profile = Some(profile.clone());
        let mut profile_typo = serde_json::to_value(&invalid).unwrap();
        profile_typo["profile"]["pace"] = 7.into();
        assert!(serde_json::from_value::<DebugScript>(profile_typo).is_err());
        assert!(invalid.validate_profile(None).is_err());
        let mut different = profile;
        different.step_ticks[0] = 8;
        assert!(invalid.validate_profile(Some(&different)).is_err());
        invalid.profile.as_mut().unwrap().step_ticks[0] = 0;
        assert!(invalid.validate().is_err());
    }
}
