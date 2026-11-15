//! Compare headless replay with the live journal's exact exported playhead.
use kn_game::{
    Game, MapRef, MapSource,
    debug::{DebugCommand, DebugJournal, DebugScript},
};
use kn_viewer::debug_replay::replay;
use std::path::PathBuf;

#[test]
fn replay_matches_exported_state_and_preserves_future_commands() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1/eng/game");
    if !root.is_dir() {
        eprintln!("skipping: user-owned ENG data absent");
        return;
    }
    let game = Game::open(root).unwrap();
    let map = MapRef {
        source: MapSource::Game,
        number: 17,
    };
    let profile = game.debug_player_profile(map, None).unwrap();
    let fresh = || {
        game.simulation_with_hero(map, 42, None)
            .map_err(|error| error.to_string())
    };
    let mut trial = fresh().unwrap();
    let cell = (0..320)
        .flat_map(|row| (0..160).map(move |column| (column, row)))
        .find(|cell| {
            trial
                .apply_debug_command(&DebugCommand::Place { cell: *cell }, Some(&profile), fresh)
                .is_ok()
        })
        .unwrap();
    let mut sim = fresh().unwrap();
    let mut journal = DebugJournal::default();
    journal
        .dispatch(
            &mut sim,
            Some(&profile),
            DebugCommand::Place { cell },
            fresh,
        )
        .result
        .unwrap();
    journal
        .seek(&mut sim, Some(&profile), 10, 64, fresh)
        .unwrap();
    journal
        .dispatch(&mut sim, None, DebugCommand::ResetAnimation, fresh)
        .result
        .unwrap();
    journal
        .seek(&mut sim, Some(&profile), 30, 64, fresh)
        .unwrap();
    journal
        .dispatch(&mut sim, None, DebugCommand::Remove, fresh)
        .result
        .unwrap();
    while !journal
        .seek(&mut sim, Some(&profile), 20, 64, fresh)
        .unwrap()
        .caught_up
    {}
    let script = DebugScript {
        version: 1,
        map,
        hero: None,
        seed: 42,
        profile: Some(profile.clone()),
        entries: journal.entries().to_vec(),
        through_tick: 20,
    };
    let report = replay(&game, &script).unwrap();
    assert_eq!(report.final_snapshot, sim.debug_snapshot());
    assert_eq!(report.script.entries, script.entries);
    assert_eq!(
        report
            .frames
            .iter()
            .filter_map(|frame| frame.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let mut authored = script.clone();
    authored.profile = None;
    for entry in &mut authored.entries {
        entry.result = None;
    }
    let report = replay(&game, &authored).unwrap();
    assert_eq!(report.final_snapshot, sim.debug_snapshot());
    assert!(report.script.entries[0].result.is_some());
    assert!(report.script.entries[2].result.is_none());
    let mut invalid = script;
    invalid.profile.as_mut().unwrap().step_ticks[0] += 1;
    assert!(
        replay(&game, &invalid)
            .unwrap_err()
            .contains("profile differs")
    );
}
