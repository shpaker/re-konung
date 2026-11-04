//! Where a roaming squad stands when the map loads.
//!
//! Epic #36 part 3, criterion 2: the game does not leave a roaming squad at
//! the cells its records carry — on map load every member is re-placed near
//! the centre of the squad's spawn area, and when the area runs out of free
//! cells the squad roster is trimmed there and the rest of the members are
//! left out. The static render keeps the old approximation (the first member
//! at the centre, `k1_data::characters::placed`); this is where the live
//! starting positions come from. The facts and our differences are in
//! docs/k1/creatures.md, «Roaming squad placement on map load».

use std::collections::{BTreeMap, HashSet};

use crate::creatures::Creature;
use crate::rules::{Ground, Random, inside};
use k1_data::game_state::Rect;

/// How many cells one member tries before giving up — the original's bound
/// (`game_character.cpp:334`).
const ATTEMPTS: u32 = 100;

/// Where every creature of a roster stands once the load is over, plus who
/// was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlacement {
    /// The load-time cell of each roster entry, indexed like the roster:
    /// a re-placed member gets its new cell, everyone else — including the
    /// members in `dropped` — keeps the cell the record carries.
    pub cells: Vec<(i16, i16)>,
    /// Roster indices of the members a squad lost for want of room, in
    /// roster order. They are out of the game: the original shrinks
    /// `GroupSize` and stops placing (`game.cpp:654-655`).
    pub dropped: Vec<usize>,
}

/// Re-place every roaming squad of `roster` the way the game does on map
/// load: squads in group-slot order, members in roster order, each member
/// tried for [`ATTEMPTS`] cells around the spawn area's centre, and the
/// squad trimmed at the first member that finds no free cell.
///
/// `ground` answers which cells a creature may stand on; taken cells are the
/// caller's business only for what this function cannot see. What it does see
/// it guards itself: a member never lands on a cell another member of any
/// squad has just been given, nor on the file cell of any creature that is
/// not re-placed — those stand where the records put them and are there when
/// the squads arrive. The original marks a cell when that creature's own
/// group reaches its turn (`game.cpp:660-664`), so holding everything else
/// from the start is our difference: what stops depending on the visit order
/// is the *position* of the creatures that are not re-placed, not the order
/// of the squads — one shared draw stream and sequentially taken cells make
/// that visible, and it mirrors the original's `MapChar_ARRAY` order
/// (`game.cpp:637`; docs/k1/creatures.md). A dead record is placed like
/// everyone else — the original places before it asks who is alive
/// (`game.cpp:651-664`).
///
/// The same seed with the same roster and ground gives the same placement;
/// the generator is only ever asked through [`Random`].
pub fn place_roaming<G: Ground, R: Random>(
    roster: &[Creature],
    ground: &G,
    random: &mut R,
) -> LoadPlacement {
    let mut cells: Vec<(i16, i16)> = roster.iter().map(|creature| creature.cell).collect();
    let mut taken: HashSet<(i16, i16)> = roster
        .iter()
        .filter(|creature| !creature.roaming)
        .map(|creature| creature.cell)
        .collect();
    let mut dropped = Vec::new();
    // Squads in group-slot order — the original walks MapChar_ARRAY, the
    // group table as the file holds it (`game.cpp:637`,
    // `game_state.cpp:364-368`); the roster's record-number sort can
    // invert slots. Members keep roster order: one squad's members are
    // contiguous in the file (`Characters.at(CharacterIndex + j)`,
    // `game.cpp:649`).
    let mut squads: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, creature) in roster.iter().enumerate() {
        if creature.roaming {
            squads.entry(creature.group).or_default().push(index);
        }
    }
    for members in squads.values() {
        let spawn = roster[members[0]].spawn;
        for position in 0..members.len() {
            let member = members[position];
            match place_one(spawn, ground, random, &taken) {
                Some(cell) => {
                    cells[member] = cell;
                    taken.insert(cell);
                }
                None => {
                    // The original's `GroupSize = j`: this member and every
                    // later one is out of the squad.
                    dropped.extend(members[position..].iter().copied());
                    break;
                }
            }
        }
    }
    LoadPlacement { cells, dropped }
}

/// One member's cell, mirroring `PlaceMob` (`game_character.cpp:328-360`):
/// jitter from the spawn centre with the original's draw order — distance
/// then coin on `y`, distance then coin on `x`, a distance drawn only when
/// its half-span reaches two. With the original's spans a candidate stays
/// strictly inside an area of span ≥ 2 and lands on the centre cell of a
/// degenerate one; `inside` (bounds included) is what the guard enforces —
/// the same containment the group-state test applies. A member of an area
/// that holds no free cell is trimmed, like one that found no room.
fn place_one<G: Ground, R: Random>(
    spawn: Rect,
    ground: &G,
    random: &mut R,
    taken: &HashSet<(i16, i16)>,
) -> Option<(i16, i16)> {
    // The centre formula is the original's and `placed`'s alike: the cell
    // the static render already draws for the squad's first member.
    let centre_x = (i64::from(spawn.right) + i64::from(spawn.left) + 1) / 2;
    let centre_y = (i64::from(spawn.bottom) + i64::from(spawn.top) + 1) / 2;
    // The original's spans are exclusive (`Width() = right - left`,
    // `common.h:187`), and so are the half-spans it draws distances from.
    let half_height = (i64::from(spawn.bottom) - i64::from(spawn.top)) / 2;
    let half_width = (i64::from(spawn.right) - i64::from(spawn.left)) / 2;
    for _ in 0..ATTEMPTS {
        let row = offset(centre_y, half_height, random);
        let column = offset(centre_x, half_width, random);
        let (Ok(column), Ok(row)) = (i16::try_from(column), i16::try_from(row)) else {
            continue;
        };
        let cell = (column, row);
        if inside(&spawn, cell) && ground.is_free(cell) && !taken.contains(&cell) {
            return Some(cell);
        }
    }
    None
}

/// Distance and coin for one axis in the original's order: the distance is
/// drawn only when the half-span reaches two, the coin always; the coin
/// picks subtract (heads) or add.
fn offset<R: Random>(centre: i64, half_span: i64, random: &mut R) -> i64 {
    let distance = if half_span >= 2 {
        i64::try_from(random.below(half_span as u64)).unwrap_or(half_span)
    } else {
        0
    };
    let subtract = random.below(2) == 0;
    if subtract {
        centre - distance
    } else {
        centre + distance
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::Creature;
    use crate::rules::{CreatureType, class, group};
    use k1_data::game_state::Rect;

    /// Walkability as a set of blocked cells; negative cells are outside
    /// the space, as every real ground reports them.
    struct Terrain {
        blocked: HashSet<(i16, i16)>,
    }

    impl Terrain {
        fn open() -> Terrain {
            Terrain {
                blocked: HashSet::new(),
            }
        }

        /// Everything of `area` blocked except the given cells.
        fn only_free(area: Rect, free: &[(i16, i16)]) -> Terrain {
            let mut blocked = HashSet::new();
            for x in area.left..=area.right {
                for y in area.top..=area.bottom {
                    blocked.insert((x, y));
                }
            }
            for cell in free {
                blocked.remove(cell);
            }
            Terrain { blocked }
        }
    }

    impl Ground for Terrain {
        fn is_free(&self, cell: (i16, i16)) -> bool {
            cell.0 >= 0 && cell.1 >= 0 && !self.blocked.contains(&cell)
        }
    }

    /// splitmix64 over a fixed seed — the engine's generator re-declared so
    /// the module tests need nothing from `kn-game`.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Rng {
            Rng(seed)
        }
    }

    impl Random for Rng {
        fn below(&mut self, limit: u64) -> u64 {
            if limit == 0 {
                return 0;
            }
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            ((((z ^ (z >> 31)) as u128) * (limit as u128)) >> 64) as u64
        }
    }

    fn area() -> Rect {
        Rect {
            left: 10,
            top: 10,
            right: 20,
            bottom: 20,
        }
    }

    fn monster(group_slot: usize, id: usize, cell: (i16, i16)) -> Creature {
        Creature {
            id,
            cell,
            direction: 0,
            state: 0,
            class_id: class::WARRIOR,
            group: group_slot,
            group_kind: group::FIRST_ROAMING,
            roaming: true,
            spawn: area(),
            workplace: None,
            sprite_set: 0,
        }
    }

    fn villager(id: usize, cell: (i16, i16)) -> Creature {
        Creature {
            class_id: class::VILLAGER,
            group: 7,
            group_kind: group::VILLAGE_FIRST,
            roaming: false,
            ..monster(0, id, cell)
        }
    }

    #[test]
    fn a_roaming_member_lands_strictly_inside_its_area_and_off_its_file_cell() {
        let file_cell = (100, 100);
        let roster = [monster(3, 5, file_cell)];
        for seed in 0..32 {
            let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(seed));
            assert!(placed.dropped.is_empty());
            let cell = placed.cells[0];
            assert_ne!(cell, file_cell, "the record's cell is not where it stands");
            let area = area();
            assert!(
                area.left < cell.0
                    && cell.0 < area.right
                    && area.top < cell.1
                    && cell.1 < area.bottom,
                "seed {seed}: {cell:?} left the interior of the area"
            );
        }
    }

    #[test]
    fn members_never_share_a_cell() {
        let roster: Vec<Creature> = (0..12).map(|n| monster(1, n, (200, 200))).collect();
        let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(4));
        let distinct: HashSet<_> = placed.cells.iter().copied().collect();
        assert_eq!(distinct.len(), roster.len());
        assert!(placed.dropped.is_empty(), "{:?}", placed.dropped);
    }

    #[test]
    fn a_member_never_lands_on_the_cell_of_a_creature_that_stays() {
        let stay = (15, 15);
        let roster = [
            villager(9, stay),
            monster(1, 10, (5, 5)),
            monster(1, 11, (6, 6)),
        ];
        for seed in 0..32 {
            let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(seed));
            assert_eq!(placed.cells[0], stay, "the villager stays put");
            assert!(
                placed.cells[1..].iter().all(|&cell| cell != stay),
                "seed {seed}: a member took the villager's cell"
            );
        }
    }

    #[test]
    fn only_free_cells_are_taken() {
        // The whole area walled but two interior cells: both members land
        // exactly on them, whoever they are.
        let free = [(13, 14), (14, 15)];
        let ground = Terrain::only_free(area(), &free);
        let roster = [monster(1, 1, (0, 0)), monster(1, 2, (0, 1))];
        let placed = place_roaming(&roster, &ground, &mut Rng::new(77));
        assert!(placed.dropped.is_empty(), "{:?}", placed.dropped);
        let mut cells = placed.cells.to_vec();
        cells.sort();
        assert_eq!(cells, free);
    }

    #[test]
    fn the_squad_is_trimmed_at_the_member_that_finds_no_room() {
        // Two free cells for three members: the third finds nothing in a
        // hundred tries and takes the end of the roster with it.
        let roster: Vec<Creature> = (0..3).map(|n| monster(1, n, (0, 0))).collect();
        let free = [(14, 14), (14, 15)];
        let ground = Terrain::only_free(area(), &free);
        let placed = place_roaming(&roster, &ground, &mut Rng::new(3));
        assert_eq!(placed.dropped, vec![2]);
        let mut cells: Vec<_> = placed.cells.iter().copied().take(2).collect();
        cells.sort();
        assert_eq!(cells, free);
        assert_eq!(
            placed.cells[2],
            (0, 0),
            "a dropped member keeps the record's cell — it is out of the game, \
             and `dropped` is what says so"
        );
    }

    #[test]
    fn the_same_seed_places_the_same_way() {
        let roster: Vec<Creature> = (0..20).map(|n| monster(n % 3, n, (0, 0))).collect();
        let first = place_roaming(&roster, &Terrain::open(), &mut Rng::new(31));
        let second = place_roaming(&roster, &Terrain::open(), &mut Rng::new(31));
        assert_eq!(first, second);
    }

    #[test]
    fn a_dead_record_is_placed_like_every_other_member() {
        // The original places before it asks who is alive (`game.cpp:651-664`).
        let mut dead = monster(1, 1, (0, 0));
        dead.state = 3;
        let roster = [monster(1, 0, (0, 0)), dead];
        let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(5));
        assert!(placed.dropped.is_empty());
        assert_ne!(placed.cells[0], placed.cells[1]);
    }

    #[test]
    fn non_roaming_creatures_cost_no_draws_and_keep_their_cells() {
        /// A generator that fails the test on any draw: the villager and the
        /// lone NPC must not touch it at all.
        struct NoDraws;
        impl Random for NoDraws {
            fn below(&mut self, limit: u64) -> u64 {
                panic!("the placement asked for below({limit})")
            }
        }
        let roster = [
            villager(1, (3, 4)),
            Creature {
                group_kind: group::LONE_NPC,
                ..villager(2, (7, 8))
            },
        ];
        let placed = place_roaming(&roster, &Terrain::open(), &mut NoDraws);
        assert_eq!(placed.cells, vec![(3, 4), (7, 8)]);
        assert!(placed.dropped.is_empty());
        assert!(matches!(
            crate::rules::classify(&roster[1]),
            CreatureType::LoneNpc
        ));
    }

    #[test]
    fn two_squads_of_one_roster_are_placed_apart() {
        let roster = [
            monster(1, 0, (0, 0)),
            monster(2, 1, (0, 0)),
            monster(1, 2, (0, 0)),
        ];
        let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(17));
        let distinct: HashSet<_> = placed.cells.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            3,
            "squads shared a cell: {:?}",
            placed.cells
        );
    }

    #[test]
    fn squads_are_placed_in_group_slot_order_not_in_record_order() {
        // The original walks `MapChar_ARRAY` — group slots ascending
        // (`game.cpp:637`) — while the roster sorts by record number, so
        // first appearance can put a later slot first. Overlapping areas
        // make the shared draw stream observable: the members of slot 1
        // must consume the first draws although they are the later
        // records.
        let roster = [
            monster(3, 0, (0, 0)),
            monster(3, 1, (0, 0)),
            monster(1, 2, (0, 0)),
            monster(1, 3, (0, 0)),
        ];
        let placed = place_roaming(&roster, &Terrain::open(), &mut Rng::new(11));
        assert!(placed.dropped.is_empty());
        let mut reference = Rng::new(11);
        let mut taken = HashSet::new();
        let mut slot_order = Vec::new();
        for _ in 0..roster.len() {
            // `expect`: the open terrain always has a free cell here.
            let cell = place_one(area(), &Terrain::open(), &mut reference, &taken).expect("open");
            taken.insert(cell);
            slot_order.push(cell);
        }
        assert_eq!(
            placed.cells,
            [slot_order[2], slot_order[3], slot_order[0], slot_order[1]],
            "the squad of the higher slot consumed the first draws"
        );
    }

    #[test]
    fn a_degenerate_area_places_on_its_only_cell_or_trims() {
        let spot = Rect {
            left: 12,
            top: 12,
            right: 12,
            bottom: 12,
        };
        let member = Creature {
            spawn: spot,
            ..monster(1, 0, (0, 0))
        };
        let placed = place_roaming(&[member], &Terrain::open(), &mut Rng::new(2));
        assert!(placed.dropped.is_empty());
        assert_eq!(placed.cells[0], (12, 12));

        let blocked = Terrain::only_free(spot, &[]);
        let placed = place_roaming(&[member], &blocked, &mut Rng::new(2));
        assert_eq!(placed.dropped, vec![0], "no free cell, no squad");
    }
}
