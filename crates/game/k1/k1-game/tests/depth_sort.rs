//! Tests for the per-object flag-8 depth-sort adjustment.
//!
//! Synthetic tests (no game data) verify the `sort_bottom` formula directly.
//! Integration tests (require `original/k1/`) verify production code end-to-end:
//! `map_scene`/`assemble` apply the adjustment (Slavik `game.cpp:1391-1395`)
//! correctly, set `depth_bottom`, and the draw order puts flag-8 objects behind
//! adjacent characters.

use std::path::PathBuf;

use k1_data::{
    game_state, map,
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    resources::Resources,
    scene::{self, Overlays, SceneInput},
};

// ---------------------------------------------------------------------------
// Synthetic tests: pure formula, no game data needed (CI-safe)
// ---------------------------------------------------------------------------

#[test]
fn sort_bottom_without_flag_is_unadjusted() {
    // position_y=200, offset_y=-50, height=100: bottom = 200-50+100 = 250
    assert_eq!(scene::sort_bottom(200, -50, 100, 0), 250);
}

#[test]
fn sort_bottom_with_flag8_reduces_by_quarter_height() {
    // Same inputs, flags=8: 250 - 100/4 = 225
    assert_eq!(scene::sort_bottom(200, -50, 100, 8), 225);
}

#[test]
fn sort_bottom_with_flag_ff_same_as_flag8() {
    // Bit 3 is what matters, other bits are irrelevant to the sort.
    assert_eq!(scene::sort_bottom(200, -50, 100, 0xFF), 225);
}

#[test]
fn sort_bottom_clamps_to_zero() {
    // position_y=0, offset_y=-200, height=100: unadjusted = -100; adjusted = -100-25 → clamp to 0
    assert_eq!(scene::sort_bottom(0, -200, 100, 8), 0);
    // Also unadjusted negative without flag stays negative (no clamp without flag).
    assert_eq!(scene::sort_bottom(0, -200, 100, 0), -100);
}

#[test]
fn sort_bottom_integer_division_truncates_toward_zero() {
    // height=7: 7/4 = 1 (integer truncation)
    // position_y=100, offset_y=0, height=7: unadjusted=107, adjusted=107-1=106
    assert_eq!(scene::sort_bottom(100, 0, 7, 8), 106);
    // height=3: 3/4 = 0 → adjustment is zero even with flag
    assert_eq!(scene::sort_bottom(100, 0, 3, 8), 103);
}

/// The specific pair from map 1 (RUS): obj 24 slot 115.
/// Independently computed: unadjusted=1849, adjusted=1716.
/// Without fix: 1849 > char_bottom(1801) → building covers character.
/// With fix: 1716 < 1801 → character draws in front.
#[test]
fn sort_bottom_fix_changes_order_for_known_building() {
    // RUS edition map 1, object_id=24, slot=115, flags=0xFF.
    let position_y = 1584; // object_position(x=28, y=99).1 = 99*16
    let offset_y = -268; // frames[base].offset.1
    let height = 533; // image.height

    let unadjusted = position_y + i32::from(offset_y) + height;
    assert_eq!(unadjusted, 1849);

    let adjusted = scene::sort_bottom(position_y, offset_y, height, 0xFF);
    assert_eq!(adjusted, 1716);
    assert_eq!(height / 4, 133); // 533/4 = 133 (integer truncation)

    // Character bottom on the same map that the fix un-hides:
    let char_bottom = 1801;
    assert!(
        unadjusted > char_bottom,
        "without fix: building covers character"
    );
    assert!(adjusted < char_bottom, "with fix: character draws in front");
}

// ---------------------------------------------------------------------------
// Integration tests: require original/k1 game data
// ---------------------------------------------------------------------------

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skip: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} exists but no edition has game/ inside: data-backed tests would skip. \
         See docs/k1/editions.md",
        root.display()
    );
    dirs
}

fn map_stacks(
    game: &GameDir,
    resources: &Resources,
    number: u32,
) -> Vec<(scene::SortKey, Vec<scene::DrawPiece>)> {
    let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
    let konung_map = map::parse(&map_bytes).unwrap();
    let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
    let lights = lights_bytes
        .as_ref()
        .map(|bytes| map::parse_lights(bytes).unwrap());
    let input = SceneInput {
        map: &konung_map,
        lights: lights.as_ref(),
        entities: None,
        state: None,
        overlays: Overlays::default(),
    };
    resources
        .with_graphics(|graph, objects| {
            scene::map_scene(&input, graph, Some(objects), &resources.tables)
                .unwrap()
                .stacks()
                .to_vec()
        })
        .unwrap()
}

/// Deterministic pair test: on map 1, the flag-8 object at slot 115 (object_id=24,
/// position_y=1584, offset_y=-268, height=533, flags=0xFF) has adjusted bottom 1716,
/// while a character at the same map has bottom 1801.
///
/// Without the fix, the unadjusted bottom (1849) > char (1801): building covers character.
/// With the fix: adjusted (1716) < char (1801): character draws in front.
///
/// The test fails if the flag-8 adjustment is disabled because the sort key
/// would become 1849, violating the hard-coded `obj_key.0 == 1716` assertion.
#[test]
fn flag8_slot115_sorts_before_adjacent_character() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes).unwrap());
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        let entities = game_state::on_map(&state, number as usize, Some(0));
        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities: Some(entities),
            state: Some(&state),
            overlays: Overlays {
                foot: false,
                lights: false,
                markers: false,
                characters: true,
            },
        };
        resources
            .with_graphics(|graph, objects| {
                let map_scene =
                    scene::map_scene(&input, graph, Some(objects), &resources.tables).unwrap();
                let poses =
                    scene::creature_poses(&input, graph, Some(objects), &resources.tables).unwrap();

                let (obj_key, _) = map_scene
                    .stacks()
                    .iter()
                    .find(|(key, pieces)| {
                        key.2 == 115 && pieces.iter().any(|p| p.depth_bottom.is_some())
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "{edition} konung.{number}: slot 115 not found among flag-8 stacks"
                        )
                    });

                assert_eq!(
                    obj_key.0, 1716,
                    "{edition} konung.{number}: slot 115 adjusted bottom must be 1716; \
                     if the flag-8 fix was removed this would be 1849"
                );

                let char_key = poses
                    .iter()
                    .map(|p| p.key)
                    .find(|k| k.0 == 1801)
                    .unwrap_or_else(|| {
                        panic!("{edition} konung.{number}: no character with bottom 1801 found")
                    });

                // With the fix: object (1716) < character (1801) → character draws in front.
                assert!(
                    obj_key < &char_key,
                    "{edition} konung.{number}: slot 115 key {obj_key:?} must sort before char key {char_key:?}"
                );

                // Compute unadjusted independently: without the fix the building covers the character.
                let placed = konung_map.objects.iter().find(|o| o.slot == 115).unwrap();
                let simple = objects.simple[placed.object_id as usize].as_ref().unwrap();
                let count = (simple.frame_count as usize).min(simple.frames.len());
                let base = (placed.frame as usize).min(count - 1);
                let image = simple.frames[base].image.as_ref().unwrap();
                let (_, py) = scene::object_position(placed.x, placed.y);
                let unadjusted = py + i32::from(simple.frames[base].offset.1) + image.height as i32;

                assert_eq!(
                    unadjusted, 1849,
                    "{edition} konung.{number}: slot 115 unadjusted bottom"
                );
                assert!(
                    unadjusted > char_key.0,
                    "{edition} slot 115: unadjusted {unadjusted} must exceed char bottom {} \
                     (otherwise the test does not exercise the fix)",
                    char_key.0
                );
            })
            .unwrap();
    }
}

/// Verify `depth_bottom` and SortKey bottom match the independent formula
/// `position_y + offset_y + height - height / 4` for a sample of objects.
#[test]
fn depth_bottom_matches_independent_formula() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let stacks = map_stacks(&game, &resources, number);
        let mut checked = 0;
        resources
            .with_graphics(|_graph, objects| {
                for (key, pieces) in &stacks {
                    let Some(depth_bottom) = pieces.first().and_then(|p| p.depth_bottom) else {
                        continue;
                    };
                    let slot = key.2;
                    let Some(placed) = konung_map.objects.iter().find(|o| o.slot == slot) else {
                        continue;
                    };
                    let Some(simple) = objects
                        .simple
                        .get(placed.object_id as usize)
                        .and_then(Option::as_ref)
                    else {
                        continue;
                    };
                    let count = (simple.frame_count as usize).min(simple.frames.len());
                    if count == 0 {
                        continue;
                    }
                    let mut base = (placed.frame as usize).min(count - 1);
                    if simple.frames[base].image.is_none() {
                        base = 0;
                    }
                    let Some(image) = simple.frames[base].image.as_ref() else {
                        continue;
                    };
                    let (_, py) = scene::object_position(placed.x, placed.y);
                    // Independent formula: not calling sort_bottom, just the arithmetic.
                    let height = image.height as i32;
                    let expected = {
                        let raw = py + i32::from(simple.frames[base].offset.1) + height;
                        let adj = raw - height / 4;
                        adj.max(0)
                    };
                    assert_eq!(
                        key.0, expected,
                        "{edition} slot {slot}: SortKey bottom {} != independent formula {expected}",
                        key.0
                    );
                    assert_eq!(
                        depth_bottom, expected,
                        "{edition} slot {slot}: depth_bottom {depth_bottom} != independent formula {expected}",
                    );
                    checked += 1;
                }
            })
            .unwrap();
        assert!(
            checked > 0,
            "{edition} konung.{number}: no flag-8 objects to verify formula"
        );
        eprintln!("{edition} konung.{number}: {checked} objects formula-verified");
    }
}

/// Flag-0 objects (per-instance) must NOT have depth_bottom. Cross-references
/// against the parsed map object's actual `flags` field.
#[test]
fn flag0_objects_have_no_depth_bottom_cross_ref_map() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let stacks = map_stacks(&game, &resources, number);
        let mut flag0_checked = 0;
        for (key, pieces) in &stacks {
            let Some(first) = pieces.first() else {
                continue;
            };
            if first.depth_bottom.is_some() || first.texture.is_none() || key.1 != 0 {
                continue;
            }
            let slot = key.2;
            let Some(placed) = konung_map.objects.iter().find(|o| o.slot == slot) else {
                continue;
            };
            assert_eq!(
                placed.flags & 8,
                0,
                "{edition} slot {slot}: object has flags={:#04x} (bit 3 set) but no depth_bottom",
                placed.flags
            );
            flag0_checked += 1;
        }
        assert!(
            flag0_checked > 0,
            "{edition} konung.{number}: no flag-0 objects to verify"
        );
    }
}

/// Clamp: no flag-8 piece has a negative sort-key bottom.
#[test]
fn flag8_sort_key_bottom_is_non_negative() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        for &number in &game.map_numbers(GAME_FILES).unwrap() {
            let stacks = map_stacks(&game, &resources, number);
            for (key, pieces) in &stacks {
                if pieces.first().is_some_and(|p| p.depth_bottom.is_some()) {
                    assert!(
                        key.0 >= 0,
                        "{edition} konung.{number}: flag-8 object has negative sort key: {:?}",
                        key
                    );
                }
            }
        }
    }
}
