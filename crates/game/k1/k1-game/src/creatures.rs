//! Who stands on a K1 map, and how long one of their steps lasts.
//!
//! Epic #36 part 3: the roster is the stable picture of a map's creatures before
//! any rule moves them. A creature's identifier is its character record number in
//! `Game.N`, so it survives a redraw of the scene and a change of cell. The data
//! comes from [k1_data::game_state] as parsed; nothing here writes files, and the
//! Python reference is `konung/k1/characters.py`.

use std::collections::HashMap;

use k1_data::characters::walk_frame_counts;
use k1_data::executable::Texts;
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
    /// Byte 0xD4 of the character record: the name table index in the
    /// executable's texts (docs/k1/formats/game-state.md, offset 0xD4).
    pub name_id: u8,
    /// Byte 0xD5 of the character record: the nickname table index; 0 — no
    /// nickname. See docs/k1/formats/game-state.md, offset 0xD4.
    pub nick_id: u8,
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
        name_id: character.name_id,
        nick_id: character.nick_id,
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

/// Frames of the state-4 special sequence at each roster creature's facing
/// direction, or 0 where the creature's sprite carries no such row. Feeds
/// [`crate::creature_anim::Seed::special_frames`] on world load.
///
/// Only the three dynamic classes 0x46, 0x4c and 0x55 record state 4 in the
/// shipped files (docs/k1/creatures.md, "Our measurement"); a human is queried
/// the same way and answers 0, so the machine never seeds it into Special.
pub fn special_frame_counts(
    state: &GameState,
    creatures: &[Creature],
    graph: &GraphRes<'_>,
    objects: &ObjectsRes<'_>,
) -> Vec<u32> {
    creatures
        .iter()
        .map(|creature| {
            state
                .characters
                .get(creature.id)
                .and_then(Option::as_ref)
                .map(|character| {
                    let mut probe = (*character).clone();
                    probe.state = crate::creature_anim::SPECIAL_STATE;
                    u32::try_from(k1_data::characters::frame_count(&probe, graph, objects))
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        })
        .collect()
}

/// Display label per record number: `#<id> <name>` (issues #38, #84). The name
/// is [`k1_data::executable::character_name`]: the species for a monster, the
/// name and the nickname for a person. A creature the tables cannot name
/// (empty tables, numbers outside them) keeps the bare `#<id>`.
///
/// Game texts live only in a window: no fixture, no CI report carries them
/// (issue #84 AC3), so the pure function is judged on synthetic tables. The
/// frontend reads the finished strings off `kn_game::simulation::Simulation`
/// and imports neither this module nor the name tables itself.
pub fn labels(roster: &[Creature], texts: Option<&Texts>) -> HashMap<usize, String> {
    roster
        .iter()
        .map(|creature| {
            let name = k1_data::executable::character_name(
                texts,
                creature.class_id,
                creature.name_id,
                creature.nick_id,
            );
            let text = if name.is_empty() {
                format!("#{}", creature.id)
            } else {
                format!("#{} {}", creature.id, name)
            };
            (creature.id, text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_read_species_for_monsters_and_name_for_people() {
        // Synthetic tables in the exact shape `parse_texts` returns: no game
        // text in any fixture, headless and edition-independent (#84 AC2/AC3).
        let texts = Texts {
            monsters: (0..16).map(|index| format!("mon{index:03}")).collect(),
            names: (0..8).map(|index| format!("name{index:03}")).collect(),
            nicknames: (0..8).map(|index| format!("nick{index:03}")).collect(),
            maps: (0..4).map(|index| format!("map{index:02}")).collect(),
            newgame: Vec::new(),
        };
        let creature = |id: usize, class_id: u8, name_id: u8, nick_id: u8| Creature {
            id,
            cell: (0, 0),
            direction: 0,
            state: 0,
            class_id,
            group: 0,
            group_kind: 0,
            roaming: false,
            spawn: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            workplace: None,
            name_id,
            nick_id,
            sprite_set: 0,
        };
        let roster = [
            // A monster (class bit 0x40, below the first villager class):
            // the label carries the species, the name numbers are unused.
            creature(7, 0x41, 0, 0),
            // A person: name and nickname joined with one space.
            creature(9, 0x51, 2, 3),
            // Nickname 0 — the game's "no nickname" marker.
            creature(11, 0x51, 5, 0),
            // Neither number lands in a table: the bare id stays.
            creature(13, 0x51, 255, 255),
        ];
        assert_eq!(
            labels(&roster, Some(&texts)),
            HashMap::from([
                (7, "#7 mon000".to_owned()),
                (9, "#9 name002 nick003".to_owned()),
                (11, "#11 name005".to_owned()),
                (13, "#13".to_owned()),
            ])
        );
        // No texts at all (an EXE without a recognizable names block): every
        // label falls back to the bare `#<id>` instead of inventing a name.
        assert_eq!(
            labels(&roster, None),
            HashMap::from([
                (7, "#7".to_owned()),
                (9, "#9".to_owned()),
                (11, "#11".to_owned()),
                (13, "#13".to_owned()),
            ])
        );
    }
}
