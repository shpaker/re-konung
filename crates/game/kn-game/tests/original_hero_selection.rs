//! A hero other than the first must drive the Scene, the simulation and the clips
//! together — all three read the same `Game.N`, or their geometry, roster and
//! clips come from different starting states (review #101, finding P2-2).
//!
//! Each hero of the original starts on its own map (`start_map`: 0→1, 1→4, 2→7),
//! so the chosen hero fields a squad on its home map that the first hero does not
//! — the discriminator that makes the assertions falsifiable.

use std::collections::BTreeSet;
use std::path::PathBuf;

use kn_game::{Game, MapSource, SceneOptions};

const SEED: u64 = 0x05EE_D101;

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = root.join(edition).join("game");
            if game.is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skip: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} exists but holds no edition with a game/ folder: the data-backed \
         test could not run. How to unpack — docs/k1/editions.md",
        root.display()
    );
    dirs
}

fn hero_options(hero: u32) -> SceneOptions {
    SceneOptions {
        characters: true,
        hero: Some(hero),
        ..SceneOptions::default()
    }
}

fn creature_slots(game: &Game, map_ref: kn_game::MapRef, hero: Option<u32>) -> BTreeSet<usize> {
    let scene = game.scene(map_ref, &hero_options(hero.unwrap())).unwrap();
    scene
        .creature_pieces
        .iter()
        .filter_map(|piece| piece.creature_id)
        .collect()
}

#[test]
fn a_non_first_hero_drives_scene_simulation_and_clips_together() {
    let roots = game_roots();
    if roots.is_empty() {
        // original/ is absent: skip the data-backed run cleanly rather than assert
        // against missing data, matching the sibling original_* tests.
        return;
    }
    let mut exercised = 0;
    for (edition, root) in roots {
        let game = Game::open(&root).unwrap();
        let heroes = game.heroes(MapSource::Game).unwrap();
        if heroes.len() < 2 {
            // An edition with a single starting hero cannot exercise the cross-hero
            // path; skip it rather than assert against absent data.
            continue;
        }
        exercised += 1;
        let first = heroes[0];
        let chosen = heroes[1];
        assert_ne!(first, chosen, "{edition}: need two distinct heroes");
        let map_ref = game.start_map(chosen); // the chosen hero's own start map

        // The chosen hero fields its squad on its home map; the first hero, whose
        // squad lives on its own (different) map, does not. If the simulation ignored
        // the hero it would load the first hero for both and these would be equal.
        let chosen_pixels = game
            .simulation_with_hero(map_ref, SEED, Some(chosen))
            .unwrap()
            .initial_pixels();
        let first_pixels = game
            .simulation_with_hero(map_ref, SEED, Some(first))
            .unwrap()
            .initial_pixels();
        assert!(
            !chosen_pixels.is_empty(),
            "{edition}: chosen hero #{chosen} must field a squad on its start map {map_ref:?}"
        );
        assert_ne!(
            chosen_pixels, first_pixels,
            "{edition}: the simulation ignored the chosen hero #{chosen} — it matches the first hero #{first}"
        );
        assert_eq!(
            game.simulation_with_hero(map_ref, SEED, None)
                .unwrap()
                .initial_pixels(),
            first_pixels,
            "{edition}: None must resolve to the first hero #{first}"
        );

        // Clips follow the same hero: the chosen hero's squad has clips, the first
        // hero on this map has a different (in practice empty) set.
        let chosen_clips: Vec<usize> = game
            .creature_animations_with_hero(map_ref, Some(chosen))
            .unwrap()
            .into_iter()
            .map(|(slot, _)| slot)
            .collect();
        let first_clips: Vec<usize> = game
            .creature_animations_with_hero(map_ref, Some(first))
            .unwrap()
            .into_iter()
            .map(|(slot, _)| slot)
            .collect();
        assert!(
            !chosen_clips.is_empty(),
            "{edition}: chosen hero #{chosen} must carry clips on {map_ref:?}"
        );
        assert_ne!(
            chosen_clips, first_clips,
            "{edition}: the clips ignored the chosen hero #{chosen}"
        );

        // And the Scene's roster geometry rides the chosen hero too — the single
        // source across all three subsystems.
        let chosen_slots = creature_slots(&game, map_ref, Some(chosen));
        let first_slots = creature_slots(&game, map_ref, Some(first));
        assert!(
            !chosen_slots.is_empty(),
            "{edition}: the chosen hero #{chosen} Scene must carry creatures"
        );
        assert_ne!(
            chosen_slots, first_slots,
            "{edition}: the Scene geometry ignored the chosen hero #{chosen}"
        );
    }
    assert!(
        exercised > 0,
        "edition data is present but no edition supplied two starting heroes: the \
         cross-hero path was never exercised"
    );
}
