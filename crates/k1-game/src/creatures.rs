//! Who stands on a K1 map, and how long one of their steps lasts.
//!
//! Epic #36 part 3: the roster is the stable picture of a map's creatures before
//! any rule moves them. A creature's identifier is its character record number in
//! `Game.N`, so it survives a redraw of the scene and a change of cell. The data
//! comes from [k1_data::game_state] as parsed; nothing here writes files, and the
//! Python reference is `konung/k1/characters.py`.

use std::collections::HashMap;

use k1_data::characters::walk_frame_counts;
use k1_data::game_state::{self, Character, GameState, Group, MapEntities, Rect};
use k1_data::graph_res::{DIRECTIONS, GraphRes};
use k1_data::objects_res::ObjectsRes;

/// Where a villager's job sends them: the place's own cell and its direction
/// (docs/k1/formats/game-state.md, the `Job` block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workplace {
    pub cell: (i16, i16),
    pub direction: u8,
    pub role: &'static str,
    /// Record number the slot points at. `0` is the game's empty-slot marker,
    /// not record 0: the hero never owns an open slot
    /// (docs/k1/creatures.md, "The villager decision").
    pub character: i16,
}

/// One creature of the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Creature {
    /// Character record number in `Game.N`: the stable identifier of the creature.
    pub id: usize,
    /// Fine-grid cell of the record. A roaming group is re-placed when the map
    /// loads, so for its members this is what the file says, not where they stand.
    pub cell: (i16, i16),
    pub direction: u8,
    pub state: u8,
    /// Byte 0x04 of the character record: class bits (work, dead flag, sprite source).
    /// See docs/k1/formats/game-state.md, `Character` offset 0x04.
    pub class_id: u8,
    pub group: usize,
    /// Group kind byte from Game.N (`kind` field): 0x20 player, 0x41–0x50 roaming,
    /// 0x53 lone NPC, 0x80–0x82 villagers. Used for behavior classification.
    /// See docs/k1/formats/game-state.md, `Group` offset 0x1E.
    pub group_kind: u8,
    pub roaming: bool,
    /// Rectangle the creature's group was put in the map for.
    pub spawn: Rect,
    pub workplace: Option<Workplace>,
    /// Byte 0xD6 of the character record: sprite set selector (GRAPH.RES models 0–2 or
    /// Objects.res dynamic objects 0–29). See docs/k1/formats/game-state.md, offset 0xD6.
    pub sprite_set: u8,
}

/// Everyone standing on `map_number`, ordered by record number.
///
/// `hero` is the `Game.N` number, so that the player group arrives on its own
/// start map ([game_state::start_map]) and nowhere else.
pub fn roster(state: &GameState, map_number: usize, hero: Option<usize>) -> Vec<Creature> {
    let entities = game_state::on_map(state, map_number, hero);
    let places = workplaces(&entities);
    let mut groups: Vec<&Group> = entities.groups.clone();
    if let Some(start) = entities.start {
        groups.push(start);
    }
    let mut creatures = Vec::new();
    for group in groups {
        for index in group.members() {
            let Some(character) = state.characters.get(index).and_then(Option::as_ref) else {
                continue;
            };
            creatures.push(creature(character, group, &places));
        }
    }
    creatures.sort_by_key(|creature| creature.id);
    creatures
}

fn creature(character: &Character, group: &Group, places: &HashMap<usize, Workplace>) -> Creature {
    Creature {
        id: character.slot,
        cell: (character.x, character.y),
        direction: character.direction,
        state: character.state,
        class_id: character.class_id,
        group: group.slot,
        group_kind: group.kind,
        roaming: group.is_roaming(),
        spawn: group.spawn,
        workplace: places.get(&character.slot).copied(),
        sprite_set: character.sprite_set,
    }
}

/// The village job slots of `map_number` in file order: index = slot, five per
/// village (docs/k1/formats/game-state.md, the `Job` block).
///
/// Our data has at most one village per map (docs/k1/creatures.md, our
/// measurement over `Game.0`–`Game.2`); were there more, the first is taken.
/// No village on the map — an empty list, and the rules find no work anywhere.
pub fn job_slots(state: &GameState, map_number: usize) -> Vec<Workplace> {
    state
        .villages
        .iter()
        .flatten()
        .find(|village| village.map_id as usize == map_number)
        .map(|village| {
            village
                .jobs
                .iter()
                .map(|job| Workplace {
                    cell: (job.x, job.y),
                    direction: job.direction,
                    role: job.role,
                    character: job.character,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Workplace of every character of this map that has one, keyed by record number.
///
/// An empty slot carries `character == 0`, the game's own test
/// (`Jobs[job].CharID != 0`, docs/k1/creatures.md, "The villager decision"):
/// keying on it as record 0 would hand the hero a workplace on every map with
/// an open slot.
fn workplaces(entities: &MapEntities) -> HashMap<usize, Workplace> {
    entities
        .jobs
        .iter()
        .filter_map(|(_, job)| {
            (job.character > 0).then_some((
                job.character as usize,
                Workplace {
                    cell: (job.x, job.y),
                    direction: job.direction,
                    role: job.role,
                    character: job.character,
                },
            ))
        })
        .collect()
}

/// How many ticks one step of each roster creature lasts, per direction: the walk
/// animation's frame count (docs/k1/movement.md, "Step duration").
///
/// The table is indexed like the roster, because parsing `GRAPH.RES` and
/// `Objects.res` costs 66 megabytes and a moving creature would otherwise ask for
/// its duration every tick.
pub fn step_durations(
    state: &GameState,
    creatures: &[Creature],
    graph: &GraphRes<'_>,
    objects: &ObjectsRes<'_>,
) -> Vec<[u32; DIRECTIONS]> {
    creatures
        .iter()
        .map(|creature| {
            let frames = state
                .characters
                .get(creature.id)
                .and_then(Option::as_ref)
                .map(|character| walk_frame_counts(character, graph, objects))
                .unwrap_or([0; DIRECTIONS]);
            frames.map(|count| u32::try_from(count).unwrap_or(u32::MAX))
        })
        .collect()
}
