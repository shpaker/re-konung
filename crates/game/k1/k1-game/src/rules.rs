//! K1 creature behavior classification and decision evaluation.
//!
//! Pure K1 logic: classifies creatures by [`CreatureType`] and produces
//! [`Decision`]s from roster data plus a live picture of the tick
//! ([`Live`], [`Ground`], [`Random`]). This module owns neither `kn-game` nor
//! `World`; application lives in kn-game where those types exist.
//!
//! Confirmed rules — docs/k1/creatures.md, "The villager decision" and "The
//! monster decision": women and children walk on a quarter roll, work comes
//! from bits 3–5 of the class or from the village slot the creature itself
//! holds, and a jobless villager walks on a half roll; a roaming squad stands
//! while its recomputed state ([`group_states`]) is calm, returns to its
//! spawn area when a member strayed, and combat is not reverse-engineered.
//! Criterion 5: lone NPCs (group kind 0x53) take no decision. The rules read
//! live positions, never file bytes.

use std::collections::HashMap;

use crate::creatures::{Creature, Workplace};
use k1_data::game_state::Rect;

/// Group kind byte from Game.N (`kind` field at offset 0x1E).
pub mod group {
    /// Player group (отряд игрока).
    pub const PLAYER: u8 = 0x20;
    /// Roaming monster groups (бродячие отряды).
    pub const FIRST_ROAMING: u8 = 0x41;
    /// Last roaming kind.
    pub const LAST_ROAMING: u8 = 0x50;
    /// Lone NPC (одиночный NPC).
    pub const LONE_NPC: u8 = 0x53;
    /// Village villager groups (жители деревень), lower 2 bits = side.
    pub const VILLAGE_FIRST: u8 = 0x80;
    /// Last documented villager group kind. `docs/k1/formats/game-state.md`
    /// records only 0x80–0x82; higher kinds are unconfirmed and not villagers.
    pub const VILLAGE_LAST: u8 = 0x82;
}

/// Class constants from Slavik (`game.h`). Byte 0x04 of `Character`.
pub mod class {
    /// Warrior (воин).
    pub const WARRIOR: u8 = 0x41;
    /// Fighter-fencer (фехтовальщик).
    pub const FENCER: u8 = 0x42;
    /// Archer (лучник).
    pub const ARCHER: u8 = 0x43;
    /// Knight-mercenary (наёмный рыцарь).
    pub const MERCENARY: u8 = 0x44;
    /// Cavalryman (конница).
    pub const CAVALRY: u8 = 0x45;
    /// Bowman (стрелок из лука).
    pub const BOWMAN: u8 = 0x46;
    /// Sellsword (ополченец).
    pub const SELLSWORD: u8 = 0x47;
    /// Mounted knight (конный наёмник).
    pub const MOUNTED: u8 = 0x48;
    /// Veteran cavalry veteran (ветеран конницы).
    pub const VETERAN_CAVALRY: u8 = 0x49;
    /// Master fencer (мастер фехтования).
    pub const MASTER_FENCER: u8 = 0x4a;
    /// Royal guard (королевская гвардия).
    pub const ROYAL_GUARD: u8 = 0x4b;
    /// Bowman-master (мастер лучников).
    pub const BOWMAN_MASTER: u8 = 0x4c;
    /// Mounted master (мастер конных наёмников).
    pub const MOUNTED_MASTER: u8 = 0x4d;
    /// Heavy infantry veteran (ветеран пехоты).
    pub const HEAVY_INFANTRY: u8 = 0x4e;
    /// Imperial guard (императорская гвардия).
    pub const IMPERIAL_GUARD: u8 = 0x4f;
    /// Village man (поселянин).
    pub const VILLAGER: u8 = 0x50;
    /// Woman (женщина).
    pub const WOMAN: u8 = 0x51;
    /// Child (ребёнок).
    pub const CHILD: u8 = 0x52;
}

/// Decision produced by one creature during evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// No new order for this tick. This does NOT stop a creature that is
    /// already walking a route from an earlier `MoveTo` — there is no stop
    /// decision yet, and `None` must not be read as "cancel".
    None,
    /// Move to the target cell: `(column, row)` of the fine grid, the same
    /// pair `Game.N` records as `(x, y)`.
    MoveTo(u32, u32),
    /// Turn in place to face this direction: the direction byte 0..7 of the
    /// original's table, the same numbering as
    /// `kn_game::movement::Direction::index()`.
    Turn(u8),
}

/// What category of creature this record represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureType {
    /// Member of a wandering monster group (kind 0x41–0x50).
    RoamingMonster,
    /// Lone NPC (group kind 0x53). Never moves.
    LoneNpc,
    /// Village villager (kind 0x80–0x82).
    Villager,
    /// Player character or warrior in a non-roaming group.
    PlayerOrWarrior,
}

/// A roster record's live picture at the tick the rules decide for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Live {
    /// Where the creature stands now — not what `Game.N` says.
    pub cell: (i16, i16),
    /// Which way it faces, or `None`: it never turned and its facing is
    /// unknown, so a decision that needs it turns the creature to be sure.
    pub direction: Option<u8>,
    /// Walking or holding a route: the decision waits for arrival. The
    /// original takes one only when flag 0x40 of byte 0x02 is set again,
    /// which the game does on arrival (`game_play.cpp:2834-2846`).
    pub busy: bool,
}

/// Where a creature may stand right now: walkable and unoccupied.
pub trait Ground {
    fn is_free(&self, cell: (i16, i16)) -> bool;
}

/// The chance rolls and cell picks of the rules.
pub trait Random {
    /// Uniform value in `0..limit`.
    fn below(&mut self, limit: u64) -> u64;
}

/// Group state byte 0x1D, recomputed by [`group_states`] — never read from
/// the file. The three meanings the game itself writes for roaming squads
/// (docs/k1/creatures.md, "The group state of a roaming squad").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupState {
    /// Calm: 0. A roaming squad stands; every other group is always this.
    Calm,
    /// 1: a live member of the player squad is inside the spawn area — the
    /// original starts combat here, which is not reverse-engineered.
    PlayerInside,
    /// 0x20: a live member of the squad stands outside the spawn area.
    MemberOutside,
}

impl GroupState {
    /// The byte of the original's state field for this meaning.
    pub const fn byte(self) -> u8 {
        match self {
            GroupState::Calm => 0,
            GroupState::PlayerInside => 1,
            GroupState::MemberOutside => 0x20,
        }
    }
}

/// Everything the rules read besides the roster; `states` and `live` are
/// indexed like `creatures`.
pub struct Context<'a, G: Ground, R: Random> {
    pub ground: &'a G,
    pub random: &'a mut R,
    /// The map's village slots, index = slot; empty when the map has no village.
    pub jobs: &'a [Workplace],
    /// The recomputed state of each creature's squad (a squad's members share
    /// it). A missing entry stands for [`GroupState::Calm`].
    pub states: &'a [GroupState],
}

/// Whether the creature still acts: the original's own test — class bit 0x80
/// is dead, states 3 and 9 are the dying and the dead (docs/k1/creatures.md).
pub fn is_alive(creature: &Creature) -> bool {
    creature.class_id & 0x80 == 0 && creature.state != 3 && creature.state != 9
}

/// Evaluate every creature and return decisions indexed like the roster.
///
/// A `None` in `live` — a creature the caller's world does not know — and an
/// entry missing altogether both give `Decision::None`, and a missing `states`
/// entry stands for [`GroupState::Calm`]. No panic and no decision from an
/// invented position: the caller's live picture may lag the roster.
pub fn evaluate<G: Ground, R: Random>(
    creatures: &[Creature],
    live: &[Option<Live>],
    context: &mut Context<'_, G, R>,
) -> Vec<Decision> {
    creatures
        .iter()
        .enumerate()
        .map(|(index, creature)| {
            let Some(live) = live.get(index).and_then(Option::as_ref) else {
                return Decision::None;
            };
            let state = context
                .states
                .get(index)
                .copied()
                .unwrap_or(GroupState::Calm);
            decision_for(creature, live, state, context)
        })
        .collect()
}

/// The group state of every creature, recomputed from live positions
/// (Slavik `game_play.cpp:1024-1062`), indexed like the roster.
///
/// Only roaming squads own a state: any live member of the player squad
/// inside the squad's spawn area makes it `PlayerInside`, else any live
/// member of the squad itself standing outside makes it `MemberOutside`,
/// else `Calm`. Every other group — villagers, the player squad, lone NPCs,
/// unknown kinds — is `Calm`. The byte stored in `Game.N` is never read: the
/// game rewrites it for roaming squads within the first ticks, and in our
/// data it is the unknown `0x80` for every squad (docs/k1/creatures.md, "The
/// group state of a roaming squad").
///
/// A creature with no live picture — a `None` entry, or none at all — is
/// `Calm` and its position joins neither scan: a cell the world never
/// reported must not decide a squad state. The original recomputes every 16
/// ticks, a 1999 cost cap we do not reproduce: this runs every tick.
///
/// The player squad is looked for in `creatures` itself, by its group kind —
/// and [`crate::creatures::roster`] lists it only on the map the game starts
/// on. Anywhere else a simulation has to add the arriving player squad to the
/// roster it passes here, or no squad is ever seen as `PlayerInside`.
pub fn group_states(creatures: &[Creature], live: &[Option<Live>]) -> Vec<GroupState> {
    // The two inputs of the scans, keyed once: live cells of the player
    // squad, and live cells per roaming squad. Dead members and members
    // without a live picture join neither.
    let mut player_cells: Vec<(i16, i16)> = Vec::new();
    let mut squad_cells: HashMap<usize, Vec<(i16, i16)>> = HashMap::new();
    for (index, creature) in creatures.iter().enumerate() {
        if !is_alive(creature) {
            continue;
        }
        let Some(live) = live.get(index).and_then(Option::as_ref) else {
            continue;
        };
        if creature.group_kind == group::PLAYER {
            player_cells.push(live.cell);
        }
        if creature.roaming {
            squad_cells
                .entry(creature.group)
                .or_default()
                .push(live.cell);
        }
    }
    creatures
        .iter()
        .enumerate()
        .map(|(index, creature)| {
            if !creature.roaming || live.get(index).and_then(Option::as_ref).is_none() {
                return GroupState::Calm;
            }
            let spawn = &creature.spawn;
            if player_cells.iter().any(|cell| inside(spawn, *cell)) {
                return GroupState::PlayerInside;
            }
            let outside = squad_cells
                .get(&creature.group)
                .is_some_and(|cells| cells.iter().any(|cell| !inside(spawn, *cell)));
            if outside {
                GroupState::MemberOutside
            } else {
                GroupState::Calm
            }
        })
        .collect()
}

fn decision_for<G: Ground, R: Random>(
    creature: &Creature,
    live: &Live,
    state: GroupState,
    context: &mut Context<'_, G, R>,
) -> Decision {
    match classify(creature) {
        CreatureType::LoneNpc => Decision::None, // criterion 5
        CreatureType::PlayerOrWarrior => Decision::None, // the original dispatches neither
        CreatureType::Villager => villager_decision(creature, live, context),
        CreatureType::RoamingMonster => monster_decision(creature, live, state, context),
    }
}

/// Classify a creature by its group kind and role.
///
/// Classification order matters: lone NPC check uses group_kind first because
/// `0x53` is a *group* value, not a character class byte.
pub fn classify(creature: &Creature) -> CreatureType {
    match creature.group_kind {
        group::LONE_NPC => CreatureType::LoneNpc,
        group::FIRST_ROAMING..=group::LAST_ROAMING => CreatureType::RoamingMonster,
        group::VILLAGE_FIRST..=group::VILLAGE_LAST => CreatureType::Villager,
        _ => {
            // Player group (0x20) or an unconfirmed kind — treated as
            // player/warrior: it does not get the villager or roaming rules.
            CreatureType::PlayerOrWarrior
        }
    }
}

/// The peaceful villager decision (Slavik `game_play.cpp:2288-2385`), in the
/// order the original itself uses: a woman or a child first, then the static
/// work from bits 3–5 of the class, then the village slot the creature holds,
/// then the half roll. `docs/k1/creatures.md`, "The villager decision".
fn villager_decision<G: Ground, R: Random>(
    creature: &Creature,
    live: &Live,
    context: &mut Context<'_, G, R>,
) -> Decision {
    if !is_alive(creature) || live.busy {
        return Decision::None;
    }
    if creature.class_id == class::WOMAN || creature.class_id == class::CHILD {
        if context.random.below(100) < 75 {
            return Decision::None;
        }
        return walk_target(creature, context);
    }
    // The static work branch belongs to whoever the slot points at, not to
    // this creature: bits 3–5 name the job, an occupied slot sends the
    // creature to join it. An empty slot falls through to the dynamic
    // workplace; the original also clears the bits in memory (`ClassID &=
    // ~(7 << 3)`), which we do not — the branch is deterministic, so the
    // next tick takes the same path (docs/k1/creatures.md).
    let job = usize::from((creature.class_id >> 3) & 7);
    if job != 0 {
        // Bits 5..7 read past the five slots in the original
        // (`std::array<Job, 5>`); with us there is no static work either way.
        if let Some(place) = context.jobs.get(job)
            && place.character != 0
        {
            return static_work(live, place, context.ground);
        }
    }
    // The dynamic workplace: the slot this creature itself holds — the
    // original's `GetVillageCharacterJob` scan, first match wins. A record
    // number past the `s16` of the slot field cannot be a holder.
    let holder = match i16::try_from(creature.id) {
        Ok(id) => context
            .jobs
            .iter()
            .find(|place| place.character > 0 && place.character == id),
        Err(_) => None,
    };
    if let Some(place) = holder {
        if live.cell == place.cell {
            return if live.direction == Some(place.direction) {
                Decision::None
            } else {
                Decision::Turn(place.direction)
            };
        }
        return move_to(place.cell);
    }
    if context.random.below(100) < 50 {
        return Decision::None;
    }
    walk_target(creature, context)
}

/// The occupied static-work branch: join the place at its first free
/// neighbour, and once adjacent face the place's direction. The original
/// returns from here — the branch never falls through.
fn static_work<G: Ground>(live: &Live, place: &Workplace, ground: &G) -> Decision {
    if !is_adjacent(live.cell, place.cell) {
        return first_free_neighbour(place.cell, ground).map_or(Decision::None, move_to);
    }
    if live.direction != Some(place.direction) {
        return Decision::Turn(place.direction);
    }
    Decision::None
}

/// A walk to a free cell strictly inside the creature's spawn area: ten
/// uniform picks, then no order. The original offsets from the creature's own
/// cell by `ViewTiles * 2` — a window-size-dependent radius
/// (`game_play.cpp:1889-1949`) we replace (docs/k1/creatures.md).
fn walk_target<G: Ground, R: Random>(
    creature: &Creature,
    context: &mut Context<'_, G, R>,
) -> Decision {
    // Interior candidates: `left + 1 ..= right - 1`, `top + 1 ..= bottom - 1`.
    let width = i64::from(creature.spawn.right) - i64::from(creature.spawn.left) - 1;
    let height = i64::from(creature.spawn.bottom) - i64::from(creature.spawn.top) - 1;
    if width <= 0 || height <= 0 {
        // A degenerate rect has no interior to pick from; no roll is spent.
        return Decision::None;
    }
    for _ in 0..10 {
        let x = i64::from(creature.spawn.left) + 1 + pick(context, width as u64);
        let y = i64::from(creature.spawn.top) + 1 + pick(context, height as u64);
        if let Some(cell) = checked_cell(x, y)
            && context.ground.is_free(cell)
        {
            return move_to(cell);
        }
    }
    Decision::None
}

/// The classes the monster decision leaves alone (`game_play.cpp:3163-3164`):
/// woman and child, and `0x53`, which the reference compares but never names.
const NOT_MONSTER: [u8; 3] = [class::WOMAN, class::CHILD, 0x53];

/// The roaming monster's decision, from its squad's recomputed state (Slavik
/// `game_play.cpp:3156-3181`, `3286`, `3337-3392`; docs/k1/creatures.md,
/// "The monster decision"). Calm stands; `PlayerInside` would start combat —
/// target choice, attack and shooting are not reverse-engineered, so we give
/// no order rather than invent one; `MemberOutside` returns. A creature of a
/// class the monster rules skip takes no decision whatever its squad state.
fn monster_decision<G: Ground, R: Random>(
    creature: &Creature,
    live: &Live,
    state: GroupState,
    context: &mut Context<'_, G, R>,
) -> Decision {
    if !is_alive(creature) || live.busy || NOT_MONSTER.contains(&creature.class_id) {
        return Decision::None;
    }
    match state {
        GroupState::Calm | GroupState::PlayerInside => Decision::None,
        GroupState::MemberOutside => monster_return(creature, live, context),
    }
}

/// The return of a strayed squad member: from the spawn area's centre, one
/// coin of `½` decides subtract or add for both axes, the jittered start is
/// checked first and then the interior is scanned row by row; the first free
/// cell becomes the order.
fn monster_return<G: Ground, R: Random>(
    creature: &Creature,
    live: &Live,
    context: &mut Context<'_, G, R>,
) -> Decision {
    let spawn = &creature.spawn;
    if inside(spawn, live.cell) {
        // Back inside the area — inclusive bounds, the original idles here.
        return Decision::None;
    }
    // The returnable band sits one cell shy of every edge; `i64` because the
    // product of the two sizes outgrows any coordinate arithmetic.
    let rh = i64::from(spawn.bottom) - i64::from(spawn.top) - 1;
    let rw = i64::from(spawn.right) - i64::from(spawn.left) - 1;
    let mut x = (i64::from(spawn.right) + i64::from(spawn.left)) / 2;
    let mut y = (i64::from(spawn.bottom) + i64::from(spawn.top)) / 2;
    // The original's draw order is the coin, then the `y` jitter, then the
    // `x` one (`game_play.cpp:3361-3375`); a jitter is drawn only when its
    // divisor reaches two, and the same coin moves both axes the same way.
    let subtract = context.random.below(2) == 0;
    if rh / 4 >= 2 {
        let jitter = pick(context, (rh / 4) as u64);
        y = if subtract { y - jitter } else { y + jitter };
    }
    if rw / 4 >= 2 {
        let jitter = pick(context, (rw / 4) as u64);
        x = if subtract { x - jitter } else { x + jitter };
    }
    for _ in 0..rh * rw {
        if let Some(cell) = checked_cell(x, y)
            && context.ground.is_free(cell)
        {
            return move_to(cell);
        }
        x += 1;
        if x >= i64::from(spawn.right) - 1 {
            x = i64::from(spawn.left) + 1;
            y += 1;
            if y >= i64::from(spawn.bottom) - 1 {
                y = i64::from(spawn.top) + 1;
            }
        }
    }
    Decision::None
}

/// One draw `below(limit)` as a coordinate offset. Every call site passes a
/// span of a spawn area, which fits the cell coordinate space — so the answer
/// does too.
fn pick<G: Ground, R: Random>(context: &mut Context<'_, G, R>, limit: u64) -> i64 {
    context.random.below(limit) as i64
}

/// Whether the cell lies inside the rect with both bounds included — the
/// original's player-proximity test (`game_play.cpp:1035-1042`). The
/// "outside" the game tests for its own members is tangled by `&&`/`||`
/// precedence into `y < top`; we use the plain negation of this test
/// (docs/k1/creatures.md). `placement` shares it for the same reason.
pub(crate) fn inside(spawn: &Rect, cell: (i16, i16)) -> bool {
    spawn.left <= cell.0 && cell.0 <= spawn.right && spawn.top <= cell.1 && cell.1 <= spawn.bottom
}

/// Neighbour offsets by direction index, for an even and an odd row — the
/// table of `kn_game::movement::Direction::offset` re-declared here because
/// `k1-game` must not depend on `kn-game` (docs/rust.md, "Creature rules").
/// Rows grow downward, odd rows are shifted right; direction 0 is west, then
/// clockwise. Public for the one test that holds the two copies together
/// (`kn-game/tests/movement_grid.rs`); the engine's table is the reference.
pub const NEIGHBOUR_OFFSETS: [((i32, i32), (i32, i32)); 8] = [
    ((-1, 0), (-1, 0)),  // west
    ((0, -1), (-1, -1)), // north-west
    ((0, -2), (0, -2)),  // north
    ((1, -1), (0, -1)),  // north-east
    ((1, 0), (1, 0)),    // east
    ((1, 1), (0, 1)),    // south-east
    ((0, 2), (0, 2)),    // south
    ((0, 1), (-1, 1)),   // south-west
];

/// The neighbour of `cell` in direction `index` (0..7), or `None` past the
/// top-left edge of the coordinate space.
fn neighbour(cell: (i16, i16), index: usize) -> Option<(i16, i16)> {
    let (even, odd) = *NEIGHBOUR_OFFSETS.get(index)?;
    let (dc, dr) = if cell.1 & 1 == 1 { odd } else { even };
    let x = i64::from(cell.0) + i64::from(dc);
    let y = i64::from(cell.1) + i64::from(dr);
    if x < 0 || y < 0 {
        return None;
    }
    checked_cell(x, y)
}

/// Back from `i64` arithmetic to cell coordinates; a value past `i16` (only
/// reachable through degenerate data) is not a cell any rule may aim at.
fn checked_cell(x: i64, y: i64) -> Option<(i16, i16)> {
    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
        return None;
    };
    Some((x, y))
}

/// Whether `from` is one of the eight neighbours of `to` — the original's
/// direction lookup `FUN_0041165c`: standing on `to` itself is NOT adjacency,
/// so a stranger on the workplace is sent to a free neighbour.
fn is_adjacent(from: (i16, i16), to: (i16, i16)) -> bool {
    (0..8).any(|index| neighbour(to, index) == Some(from))
}

/// The first free neighbour of `cell` in direction order 0..7 — the original
/// tests terrain only (`FUN_00410ffc`); with us nobody may stand there
/// either (docs/k1/creatures.md).
fn first_free_neighbour<G: Ground>(cell: (i16, i16), ground: &G) -> Option<(i16, i16)> {
    (0..8)
        .filter_map(|index| neighbour(cell, index))
        .find(|next| ground.is_free(*next))
}

/// The cell as a `MoveTo` target; a negative coordinate is not an order the
/// engine could execute, so it means no order.
fn move_to(cell: (i16, i16)) -> Decision {
    match (u32::try_from(cell.0), u32::try_from(cell.1)) {
        (Ok(column), Ok(row)) => Decision::MoveTo(column, row),
        _ => Decision::None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A `Ground` over a set of blocked or already-occupied cells.
    #[derive(Default)]
    struct MockGround(HashSet<(i16, i16)>);

    impl Ground for MockGround {
        fn is_free(&self, cell: (i16, i16)) -> bool {
            !self.0.contains(&cell)
        }
    }

    /// Fixed `below` answers; a call past the script fails the test, which is
    /// how "this branch rolls no dice" gets asserted.
    struct ScriptedRandom {
        draws: Vec<u64>,
    }

    impl ScriptedRandom {
        fn new(draws: impl IntoIterator<Item = u64>) -> ScriptedRandom {
            ScriptedRandom {
                draws: draws.into_iter().collect(),
            }
        }
    }

    impl Random for ScriptedRandom {
        fn below(&mut self, limit: u64) -> u64 {
            if self.draws.is_empty() {
                panic!("script exhausted: asked for below({limit})");
            }
            let value = self.draws.remove(0);
            assert!(value < limit, "scripted draw {value} is not below {limit}");
            value
        }
    }

    fn make_creature(group_kind: u8, id: usize) -> Creature {
        make(group_kind, 0, id)
    }

    fn make(group_kind: u8, class_id: u8, id: usize) -> Creature {
        Creature {
            id,
            cell: (10, 10),
            direction: 0,
            state: 0,
            class_id,
            group: 0,
            group_kind,
            roaming: matches!(group_kind, group::FIRST_ROAMING..=group::LAST_ROAMING),
            spawn: k1_data::game_state::Rect {
                left: 0,
                top: 0,
                right: 50,
                bottom: 50,
            },
            workplace: None,
            sprite_set: 0,
        }
    }

    fn villager(class_id: u8, id: usize) -> Creature {
        make(group::VILLAGE_FIRST, class_id, id)
    }

    fn standing(cell: (i16, i16)) -> Live {
        Live {
            cell,
            direction: Some(0),
            busy: false,
        }
    }

    /// Every creature known to the world — the picture a caller wraps in
    /// `Option` because the world may not know some of them.
    fn pictures(live: &[Live]) -> Vec<Option<Live>> {
        live.iter().copied().map(Some).collect()
    }

    fn job(character: i16, cell: (i16, i16), direction: u8) -> Workplace {
        Workplace {
            cell,
            direction,
            role: "test",
            character,
        }
    }

    /// One creature decided against a given ground, script and job list.
    fn decide_with(
        creature: &Creature,
        live: &Live,
        ground: &MockGround,
        draws: impl IntoIterator<Item = u64>,
        jobs: &[Workplace],
    ) -> Decision {
        decide_all(
            std::slice::from_ref(creature),
            std::slice::from_ref(live),
            ground,
            draws,
            jobs,
        )[0]
    }

    /// Several creatures in one pass, decisions in roster order.
    fn decide_all(
        creatures: &[Creature],
        live: &[Live],
        ground: &MockGround,
        draws: impl IntoIterator<Item = u64>,
        jobs: &[Workplace],
    ) -> Vec<Decision> {
        decide_all_with_states(creatures, live, ground, draws, jobs, &[])
    }

    fn decide_all_with_states(
        creatures: &[Creature],
        live: &[Live],
        ground: &MockGround,
        draws: impl IntoIterator<Item = u64>,
        jobs: &[Workplace],
        states: &[GroupState],
    ) -> Vec<Decision> {
        let live: Vec<Option<Live>> = live.iter().copied().map(Some).collect();
        decide_optional(creatures, &live, ground, draws, jobs, states)
    }

    /// The same, with an explicit `None` where the world does not know a
    /// creature — the picture [`kn_game::rules::live`] hands over.
    fn decide_optional(
        creatures: &[Creature],
        live: &[Option<Live>],
        ground: &MockGround,
        draws: impl IntoIterator<Item = u64>,
        jobs: &[Workplace],
        states: &[GroupState],
    ) -> Vec<Decision> {
        let mut random = ScriptedRandom::new(draws);
        let mut context = Context {
            ground,
            random: &mut random,
            jobs,
            states,
        };
        evaluate(creatures, live, &mut context)
    }

    /// One roaming monster at the given squad state, everything else empty.
    fn decide_monster(
        creature: &Creature,
        live: &Live,
        state: GroupState,
        ground: &MockGround,
        draws: impl IntoIterator<Item = u64>,
    ) -> Decision {
        decide_all_with_states(
            std::slice::from_ref(creature),
            std::slice::from_ref(live),
            ground,
            draws,
            &[],
            std::slice::from_ref(&state),
        )[0]
    }

    fn decide(creature: &Creature, live: &Live, draws: impl IntoIterator<Item = u64>) -> Decision {
        decide_with(creature, live, &MockGround::default(), draws, &[])
    }

    // ── Classification tests (by independent group_kind values) ──

    #[test]
    fn group_0x53_is_lone_npc() {
        assert_eq!(
            classify(&make_creature(group::LONE_NPC, 1)),
            CreatureType::LoneNpc
        );
    }

    #[test]
    fn group_0x20_is_player_or_warrior() {
        assert_eq!(
            classify(&make_creature(group::PLAYER, 1)),
            CreatureType::PlayerOrWarrior
        );
    }

    #[test]
    fn group_0x41_to_0x50_are_roaming_monsters() {
        for kind in group::FIRST_ROAMING..=group::LAST_ROAMING {
            assert_eq!(
                classify(&make_creature(kind, 0)),
                CreatureType::RoamingMonster
            );
        }
    }

    #[test]
    fn group_0x80_to_0x82_are_villagers() {
        for kind in group::VILLAGE_FIRST..=group::VILLAGE_LAST {
            assert_eq!(classify(&make_creature(kind, 0)), CreatureType::Villager);
        }
    }

    /// Boundary of the confirmed villager range: 0x83 is not documented as a
    /// villager kind, so it must not be classified as one.
    #[test]
    fn group_just_above_villager_range_is_not_a_villager() {
        assert_eq!(
            classify(&make_creature(group::VILLAGE_LAST + 1, 0)),
            CreatureType::PlayerOrWarrior
        );
    }

    /// Group_kind boundaries on either side of every classified range.
    #[test]
    fn group_kind_boundaries() {
        // Just below / at / above the roaming range 0x41..=0x50.
        assert_eq!(
            classify(&make_creature(group::FIRST_ROAMING - 1, 0)),
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(group::FIRST_ROAMING, 0)),
            CreatureType::RoamingMonster
        );
        assert_eq!(
            classify(&make_creature(group::LAST_ROAMING, 0)),
            CreatureType::RoamingMonster
        );
        assert_eq!(
            classify(&make_creature(group::LAST_ROAMING + 1, 0)),
            CreatureType::PlayerOrWarrior
        );
        // Just below / at the lone NPC 0x53.
        assert_eq!(
            classify(&make_creature(group::LONE_NPC - 1, 0)),
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(group::LONE_NPC, 0)),
            CreatureType::LoneNpc
        );
        // Just below / at the villager range 0x80.
        assert_eq!(
            classify(&make_creature(group::VILLAGE_FIRST - 1, 0)),
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(group::VILLAGE_FIRST, 0)),
            CreatureType::Villager
        );
    }

    // Negative: unknown group kinds fall through to player_or_warrior.
    #[test]
    fn unknown_group_kinds_default_to_player_or_warrior() {
        assert_eq!(
            classify(&make_creature(0, 0)), // empty slot
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(0x10, 0)), // low value
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(0xff, 0)), // above every range
            CreatureType::PlayerOrWarrior
        );
    }

    /// Independence: `class_id` never changes the classification outcome —
    /// the decision reads the class byte, the classification does not.
    #[test]
    fn classification_ignores_class_id() {
        for class_id in [0x00u8, class::WARRIOR, class::WOMAN, class::CHILD, 0xff] {
            assert_eq!(
                classify(&make(group::LONE_NPC, class_id, 0)),
                CreatureType::LoneNpc,
                "class {class_id:#04x} must not change LoneNpc"
            );
            assert_eq!(
                classify(&make(group::VILLAGE_FIRST, class_id, 0)),
                CreatureType::Villager,
                "class {class_id:#04x} must not change Villager"
            );
            assert_eq!(
                classify(&make(group::FIRST_ROAMING, class_id, 0)),
                CreatureType::RoamingMonster,
                "class {class_id:#04x} must not change RoamingMonster"
            );
        }
    }

    // ── Entry gates (criterion 5 and the shared skips) ──

    #[test]
    fn lone_npc_gets_no_movement_decision() {
        // Criterion 5: lone NPCs (group kind 0x53) take no movement decisions.
        let npc = make_creature(group::LONE_NPC, 42);
        let decisions = decide_all(
            &[npc],
            &[standing((10, 10))],
            &MockGround::default(),
            [],
            &[],
        );
        assert_eq!(decisions.len(), 1);
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn missing_state_entry_means_calm() {
        // A monster whose squad state the caller never supplied stands:
        // missing `states` is Calm, not a panic.
        let monster = make_creature(group::FIRST_ROAMING, 100);
        let decisions = decide_all(
            &[monster],
            &[standing((10, 10))],
            &MockGround::default(),
            [],
            &[],
        );
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn empty_roster_returns_empty_decisions() {
        let decisions = decide_all(&[], &[], &MockGround::default(), [], &[]);
        assert!(decisions.is_empty());
    }

    #[test]
    fn missing_live_entry_decides_none_not_panic() {
        let creatures = [villager(class::VILLAGER, 1), villager(class::VILLAGER, 2)];
        let live = [standing((5, 5))];
        // Creature 1 rolls and stands (49 < 50); creature 2 has no live
        // picture, so it neither rolls nor panics.
        let decisions = decide_all(&creatures, &live, &MockGround::default(), [49], &[]);
        assert_eq!(decisions[0], Decision::None);
        assert_eq!(decisions[1], Decision::None);
    }

    /// A creature the world does not know — the explicit `None`
    /// `kn_game::rules::live` reports for an absent id — decides nothing and
    /// rolls no dice, instead of standing at an invented cell.
    #[test]
    fn a_creature_the_world_does_not_know_decides_nothing() {
        let creatures = [villager(class::VILLAGER, 1), villager(class::VILLAGER, 2)];
        let live = [Some(standing((5, 5))), None];
        // Creature 1 stands on the half roll (49 < 50); creature 2 has no
        // picture at all, and the exhausted script proves it never rolled.
        let decisions = decide_optional(&creatures, &live, &MockGround::default(), [49], &[], &[]);
        assert_eq!(decisions, vec![Decision::None, Decision::None]);
    }

    #[test]
    fn dead_or_busy_villagers_get_no_decision() {
        let ground = MockGround::default();
        let dead_class = villager(0x80 | class::VILLAGER, 1);
        let mut dead_state = villager(class::VILLAGER, 2);
        dead_state.state = 3;
        let mut buried = villager(class::VILLAGER, 3);
        buried.state = 9;
        let busy = villager(class::VILLAGER, 4);
        for (creature, live) in [
            (&dead_class, standing((5, 5))),
            (&dead_state, standing((5, 5))),
            (&buried, standing((5, 5))),
            (
                &busy,
                Live {
                    cell: (5, 5),
                    direction: Some(0),
                    busy: true,
                },
            ),
        ] {
            // An empty script: a roll anywhere would abort the test.
            let decision = decide_with(creature, &live, &ground, [], &[]);
            assert_eq!(decision, Decision::None, "creature {}", creature.id);
        }
    }

    #[test]
    fn is_alive_follows_the_originals_test() {
        let mut alive = villager(class::VILLAGER, 1);
        assert!(is_alive(&alive));
        for state in [3, 9] {
            alive.state = state;
            assert!(!is_alive(&alive), "state {state} is death");
        }
        alive.state = 6;
        alive.class_id = 0x80 | class::VILLAGER;
        assert!(!is_alive(&alive), "class bit 0x80 is death");
        // Other states stay alive: hit reaction and attacks included.
        alive.class_id = class::VILLAGER;
        alive.state = 2;
        assert!(is_alive(&alive));
        alive.state = 8;
        assert!(is_alive(&alive));
    }

    // ── Criterion 3: the woman and child roll ──

    #[test]
    fn woman_stays_below_the_walk_roll() {
        // below(100) draws 74 of 100: the three-quarters that stand (25 % walk).
        let woman = villager(class::WOMAN, 7);
        assert_eq!(decide(&woman, &standing((10, 10)), [74]), Decision::None);
    }

    #[test]
    fn woman_walks_on_the_quarter_roll() {
        let woman = villager(class::WOMAN, 7);
        // 75 of 100 crosses the quarter: walk, then the pick x = 0+1+1, y = 0+1+2.
        assert_eq!(
            decide(&woman, &standing((10, 10)), [75, 1, 2]),
            Decision::MoveTo(2, 3),
        );
    }

    #[test]
    fn child_uses_the_same_quarter_roll() {
        let child = villager(class::CHILD, 8);
        assert_eq!(decide(&child, &standing((10, 10)), [49]), Decision::None);
        assert_eq!(
            decide(&child, &standing((10, 10)), [80, 3, 4]),
            Decision::MoveTo(4, 5),
        );
    }

    // ── Criterion 3: the static work from the class bits ──

    /// Class with work bits 3–5 equal to `job`, staying inside every other
    /// rule of the class byte (not dead, not woman or child).
    fn worker(job: u8, id: usize) -> Creature {
        villager(job << 3, id)
    }

    #[test]
    fn occupied_slot_sends_to_the_first_free_neighbour() {
        // Bits name slot 2; slot 2 belongs to character 5 at (20, 20).
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        // Not adjacent: the west neighbour of the place, no dice rolled.
        let decision = decide_with(
            &creature,
            &standing((0, 0)),
            &MockGround::default(),
            [],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(19, 20));
    }

    #[test]
    fn occupied_slot_walks_to_the_next_neighbour_when_the_first_is_taken() {
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        // (19, 20) west is taken; row 20 is even, so north-west is (20, 19).
        let ground = MockGround([(19, 20)].into());
        let decision = decide_with(&creature, &standing((0, 0)), &ground, [], &jobs);
        assert_eq!(decision, Decision::MoveTo(20, 19));
    }

    #[test]
    fn no_free_neighbour_of_the_place_is_no_order() {
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        // Block all eight neighbours of (20, 20); row 20 is even.
        let ring: HashSet<(i16, i16)> = (0..8)
            .filter_map(|index| neighbour((20, 20), index))
            .collect();
        assert_eq!(ring.len(), 8);
        let ground = MockGround(ring);
        let decision = decide_with(&creature, &standing((0, 0)), &ground, [], &jobs);
        assert_eq!(
            decision,
            Decision::None,
            "the original walks to garbage here"
        );
    }

    #[test]
    fn adjacent_to_the_place_the_villager_faces_it() {
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        let live = Live {
            cell: (19, 20), // the west neighbour of the place
            direction: Some(0),
            busy: false,
        };
        let decision = decide_with(&creature, &live, &MockGround::default(), [], &jobs);
        assert_eq!(decision, Decision::Turn(5));
    }

    #[test]
    fn adjacent_and_already_facing_the_place_stands() {
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        let live = Live {
            cell: (19, 20),
            direction: Some(5),
            busy: false,
        };
        let decision = decide_with(&creature, &live, &MockGround::default(), [], &jobs);
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn standing_on_the_place_itself_is_not_adjacency() {
        // `FUN_0041165c` only reports neighbours: the stranger on the cell is
        // sent to a free neighbour, never told to face.
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(5, (20, 20), 5),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        let decision = decide_with(
            &creature,
            &standing((20, 20)),
            &MockGround::default(),
            [],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(19, 20));
    }

    #[test]
    fn empty_slot_falls_through_to_the_half_roll() {
        // Slot 2 is empty (`character == 0`): no static work, and character 9
        // holds no slot either, so the jobless half roll decides.
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(0, (30, 30), 4),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        // 49 of 100 stands: below(100) < 50.
        let decision = decide_with(
            &creature,
            &standing((0, 0)),
            &MockGround::default(),
            [49],
            &jobs,
        );
        assert_eq!(decision, Decision::None);
        // 50 walks to the picked cell x = 0+1+2, y = 0+1+3.
        let decision = decide_with(
            &creature,
            &standing((0, 0)),
            &MockGround::default(),
            [50, 2, 3],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(3, 4));
    }

    #[test]
    fn empty_slot_still_yields_to_a_dynamic_workplace() {
        // Slot 2 is empty; slot 4 belongs to character 9 itself.
        let creature = worker(2, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(0, (30, 30), 4),
            job(0, (0, 0), 0),
            job(9, (40, 41), 6),
        ];
        let decision = decide_with(
            &creature,
            &standing((0, 0)),
            &MockGround::default(),
            [],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(40, 41));
    }

    #[test]
    fn work_bits_past_the_five_slots_are_no_static_work() {
        // Bits of 5, 6 and 7 read past the array in the original; here the
        // creature falls through to the half roll with no static order.
        // Character 99 holds no slot either, so the roll is reached.
        for bits in [5u8, 6, 7] {
            let creature = worker(bits, 99);
            let jobs = [
                job(5, (10, 10), 1),
                job(6, (11, 11), 2),
                job(7, (12, 12), 3),
                job(8, (13, 13), 4),
                job(9, (14, 14), 5),
            ];
            // The standing roll proves the fall-through: the occupied-slot
            // branch never rolls, and below(100) = 0 keeps the creature home.
            let decision = decide_with(
                &creature,
                &standing((0, 0)),
                &MockGround::default(),
                [0],
                &jobs,
            );
            assert_eq!(decision, Decision::None, "bits {bits} must fall through");
        }
    }

    // ── Criterion 3: the dynamic workplace ──

    #[test]
    fn dynamic_workplace_sends_onto_the_place_cell_exactly() {
        let creature = villager(class::VILLAGER, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(9, (33, 22), 3),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        let decision = decide_with(
            &creature,
            &standing((1, 1)),
            &MockGround::default(),
            [],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(33, 22));
    }

    #[test]
    fn on_the_place_cell_the_villager_turns_to_its_direction() {
        let creature = villager(class::VILLAGER, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(9, (33, 22), 6),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
            job(0, (0, 0), 0),
        ];
        let wrong = Live {
            cell: (33, 22),
            direction: Some(2),
            busy: false,
        };
        assert_eq!(
            decide_with(&creature, &wrong, &MockGround::default(), [], &jobs),
            Decision::Turn(6)
        );
        // An unknown facing is not the place's direction: it turns to be sure.
        let unknown = Live {
            cell: (33, 22),
            direction: None,
            busy: false,
        };
        assert_eq!(
            decide_with(&creature, &unknown, &MockGround::default(), [], &jobs),
            Decision::Turn(6)
        );
        let facing = Live {
            cell: (33, 22),
            direction: Some(6),
            busy: false,
        };
        assert_eq!(
            decide_with(&creature, &facing, &MockGround::default(), [], &jobs),
            Decision::None
        );
    }

    #[test]
    fn first_slot_wins_when_two_hold_the_same_creature() {
        // `GetVillageCharacterJob` scans slots in order; the data gives each
        // holder one slot, and we keep the original's first match.
        let creature = villager(class::VILLAGER, 9);
        let jobs = [
            job(0, (0, 0), 0),
            job(9, (10, 10), 1),
            job(0, (0, 0), 0),
            job(9, (20, 20), 2),
            job(0, (0, 0), 0),
        ];
        let decision = decide_with(
            &creature,
            &standing((1, 1)),
            &MockGround::default(),
            [],
            &jobs,
        );
        assert_eq!(decision, Decision::MoveTo(10, 10));
    }

    // ── Criterion 3: the jobless roll and the walk ──

    #[test]
    fn jobless_villager_stands_below_the_half_roll() {
        // Class 5 carries no work bits, and no slot points at character 9.
        let creature = villager(5, 9);
        assert_eq!(decide(&creature, &standing((10, 10)), [49]), Decision::None);
    }

    #[test]
    fn jobless_villager_walks_at_the_half_roll() {
        let creature = villager(5, 9);
        assert_eq!(
            decide(&creature, &standing((10, 10)), [50, 6, 7]),
            Decision::MoveTo(7, 8),
        );
    }

    #[test]
    fn walk_target_is_strictly_inside_and_free() {
        // Spawn rect (10, 10)..(14, 14): interior 11..=13 on both axes; the
        // first pick lands on a blocked cell, the second is accepted.
        let mut creature = villager(5, 9);
        creature.spawn = k1_data::game_state::Rect {
            left: 10,
            top: 10,
            right: 14,
            bottom: 14,
        };
        let ground = MockGround([(11, 11)].into());
        let decision = decide_with(
            &creature,
            &standing((12, 12)),
            &ground,
            [50, 0, 0, 1, 0],
            &[],
        );
        assert_eq!(decision, Decision::MoveTo(12, 11));
    }

    #[test]
    fn walk_gives_up_after_ten_taken_attempts() {
        let mut creature = villager(5, 9);
        creature.spawn = k1_data::game_state::Rect {
            left: 0,
            top: 0,
            right: 4,
            bottom: 4,
        };
        // Interior 1..=3 on both axes, all nine cells taken: ten picks miss.
        let ground = MockGround(
            (1..4)
                .flat_map(|x| (1..4).map(move |y| (x, y)))
                .collect::<HashSet<_>>(),
        );
        let draws = std::iter::once(50)
            .chain((0..10).flat_map(|_| [0, 0]))
            .collect::<Vec<_>>();
        let decision = decide_with(&creature, &standing((2, 2)), &ground, draws, &[]);
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn degenerate_spawn_rect_walks_nowhere_without_picking() {
        for (right, bottom) in [(1i16, 50i16), (50, 1), (1, 1), (0, 0)] {
            let mut creature = villager(5, 9);
            creature.spawn.right = right;
            creature.spawn.bottom = bottom;
            // The half roll passes (50); no pick is drawn — a draw would
            // abort the test through the exhausted script.
            let decision = decide(&creature, &standing((0, 0)), [50]);
            assert_eq!(decision, Decision::None, "rect to ({right}, {bottom})");
        }
    }

    #[test]
    fn mixed_roster_indexed_correctly() {
        let creatures = vec![
            make_creature(group::LONE_NPC, 1),      // None
            make_creature(group::FIRST_ROAMING, 2), // no state supplied: calm, stands
            make_creature(group::VILLAGE_FIRST, 3), // below the half roll: stands
        ];
        let live = vec![standing((10, 10)); 3];
        let decisions = decide_all(&creatures, &live, &MockGround::default(), [49], &[]);
        assert_eq!(decisions.len(), 3);
        for decision in &decisions {
            assert!(matches!(decision, Decision::None));
        }
    }

    // ── Criterion 4: the recomputed squad state ──

    /// A spawn area from 10 to 20 on both axes, both bounds inside.
    fn spawn_rect() -> k1_data::game_state::Rect {
        k1_data::game_state::Rect {
            left: 10,
            top: 10,
            right: 20,
            bottom: 20,
        }
    }

    fn roaming_at(squad: usize, id: usize) -> Creature {
        let mut creature = make(group::FIRST_ROAMING, class::WARRIOR, id);
        creature.group = squad;
        creature.spawn = spawn_rect();
        creature
    }

    fn player_at(id: usize) -> Creature {
        let mut creature = make(group::PLAYER, class::WARRIOR, id);
        creature.spawn = spawn_rect();
        creature
    }

    #[test]
    fn group_state_bytes_match_the_original() {
        assert_eq!(GroupState::Calm.byte(), 0);
        assert_eq!(GroupState::PlayerInside.byte(), 1);
        assert_eq!(GroupState::MemberOutside.byte(), 0x20);
    }

    #[test]
    fn squad_inside_with_no_player_is_calm() {
        let squad = [roaming_at(3, 1), roaming_at(3, 2)];
        let live = [standing((11, 11)), standing((20, 20))]; // 20 is on the edge: inside
        assert_eq!(
            group_states(&squad, &pictures(&live)),
            vec![GroupState::Calm, GroupState::Calm]
        );
    }

    #[test]
    fn strayed_member_marks_the_squad_outside() {
        let squad = [roaming_at(3, 1), roaming_at(3, 2)];
        // One member steps one cell past the right edge (20): outside.
        let live = [standing((12, 12)), standing((21, 12))];
        assert_eq!(
            group_states(&squad, &pictures(&live)),
            vec![GroupState::MemberOutside, GroupState::MemberOutside]
        );
    }

    #[test]
    fn player_inside_the_area_outweighs_a_strayed_member() {
        // The original checks the player first: with both true, the squad
        // enters combat mode, not the return.
        let roster = [roaming_at(3, 1), player_at(2)];
        let live = [standing((30, 30)), standing((15, 15))];
        assert_eq!(
            group_states(&roster, &pictures(&live)),
            vec![GroupState::PlayerInside, GroupState::Calm]
        );
    }

    #[test]
    fn dead_members_join_neither_scan() {
        let stray = roaming_at(3, 1); // stands inside itself
        let mut dead_stray = roaming_at(3, 2);
        dead_stray.state = 3; // the strayed one is dying: not `MemberOutside`
        let mut dead_player = player_at(3);
        dead_player.class_id = 0x80 | class::WARRIOR; // inside the area but dead: no `PlayerInside`
        let roster = [stray, dead_stray, dead_player];
        let live = [standing((12, 12)), standing((40, 40)), standing((15, 15))];
        assert_eq!(
            group_states(&roster, &pictures(&live)),
            vec![GroupState::Calm, GroupState::Calm, GroupState::Calm]
        );
    }

    #[test]
    fn non_roaming_groups_are_calm_wherever_they_stand() {
        // Villager, lone NPC and player squads far outside their areas stay
        // Calm. `Creature` carries no `ai_state` byte at all — and in
        // `Game.0`–`Game.2` every squad's stored value is the unknown 0x80 —
        // so the file byte cannot leak into any result.
        let mut villager_squad = villager(class::VILLAGER, 1);
        villager_squad.spawn = spawn_rect();
        let npc = make_creature(group::LONE_NPC, 2);
        let hero = player_at(3);
        let roster = [villager_squad, npc, hero];
        let live = [standing((40, 40)), standing((40, 40)), standing((40, 40))];
        assert_eq!(
            group_states(&roster, &pictures(&live)),
            vec![GroupState::Calm, GroupState::Calm, GroupState::Calm]
        );
    }

    #[test]
    fn missing_live_entry_is_calm_and_joins_no_scan() {
        let squad = [roaming_at(3, 1), roaming_at(3, 2)];
        // Creature 2 has no live picture; 1 stands inside — nobody is seen
        // strayed, and 2 itself reads Calm.
        let live = [standing((12, 12))];
        assert_eq!(
            group_states(&squad, &pictures(&live)),
            vec![GroupState::Calm, GroupState::Calm]
        );
    }

    /// The regression the explicit `None` exists for: a squad whose spawn area
    /// covers the origin must not read a player member the world does not know
    /// as standing inside it, which is what an invented `(0, 0)` cell did.
    #[test]
    fn an_unknown_player_member_never_makes_a_squad_hostile() {
        let origin_area = k1_data::game_state::Rect {
            left: -5,
            top: -5,
            right: 5,
            bottom: 5,
        };
        let mut monster = roaming_at(3, 1);
        monster.spawn = origin_area;
        let mut hero = player_at(2);
        hero.spawn = origin_area;
        // The squad member stands inside its own area; the hero is unknown.
        let live = [Some(standing((0, 0))), None];
        assert_eq!(
            group_states(&[monster, hero], &live),
            vec![GroupState::Calm, GroupState::Calm],
            "an absent live picture must not place anybody at the origin"
        );
    }

    #[test]
    fn two_squads_never_share_a_state() {
        // Each roaming squad's state uses its own area and its own members.
        let one = roaming_at(3, 1);
        let two = roaming_at(4, 2);
        let live = [standing((12, 12)), standing((12, 12))];
        assert_eq!(
            group_states(&[one, two], &pictures(&live)),
            vec![GroupState::Calm, GroupState::Calm]
        );
    }

    // ── Criterion 4: the monster decision ──

    /// An area big enough for the return band but too small to jitter:
    /// `rh = rw = 6`, so `6 / 4 < 2` and no jitter is drawn at all.
    fn small_squad() -> Creature {
        let mut monster = roaming_at(3, 1);
        monster.spawn = k1_data::game_state::Rect {
            left: 0,
            top: 0,
            right: 7,
            bottom: 7,
        };
        monster
    }

    #[test]
    fn calm_monster_stands_without_rolling() {
        let monster = small_squad();
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::Calm,
            &MockGround::default(),
            [],
        );
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn player_inside_stands_because_combat_is_not_recovered() {
        // `PlayerInside` in the original starts combat — target choice,
        // attack, shooting — none of it reverse-engineered. We give no
        // order rather than invent one (docs/k1/creatures.md).
        let monster = small_squad();
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::PlayerInside,
            &MockGround::default(),
            [],
        );
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn member_outside_while_already_inside_the_area_stands() {
        // The squad is flagged by another strayed member; this one is back
        // on the edge cell, inside with both bounds counted.
        let monster = small_squad();
        let decision = decide_monster(
            &monster,
            &standing((7, 0)),
            GroupState::MemberOutside,
            &MockGround::default(),
            [],
        );
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn strayed_monster_returns_to_the_centre_of_the_area() {
        let monster = small_squad();
        // Coin 0 (subtract); neither axis jitters, so the scan starts at the
        // centre ((7 + 0) / 2, (7 + 0) / 2) = (3, 3) and finds it free.
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::MemberOutside,
            &MockGround::default(),
            [0],
        );
        assert_eq!(decision, Decision::MoveTo(3, 3));
    }

    #[test]
    fn one_coin_moves_both_axes_the_same_way() {
        // Area 20 × 20: `rh = rw = 19`, jitter divisor `19 / 4 = 4 ≥ 2` on
        // both axes; centre (10, 10). Draw order: coin, y, x.
        let mut monster = roaming_at(3, 1);
        monster.spawn = k1_data::game_state::Rect {
            left: 0,
            top: 0,
            right: 20,
            bottom: 20,
        };
        let add = decide_monster(
            &monster,
            &standing((40, 40)),
            GroupState::MemberOutside,
            &MockGround::default(),
            [1, 2, 3],
        );
        assert_eq!(add, Decision::MoveTo(13, 12), "coin 1 adds to both axes");
        let subtract = decide_monster(
            &monster,
            &standing((40, 40)),
            GroupState::MemberOutside,
            &MockGround::default(),
            [0, 2, 3],
        );
        assert_eq!(
            subtract,
            Decision::MoveTo(7, 8),
            "coin 0 subtracts from both axes"
        );
    }

    #[test]
    fn taken_cells_are_skipped_and_the_scan_wraps_the_row() {
        let monster = small_squad();
        // Centre (3, 3) and the next two cells east are taken; after (5, 3)
        // the x step crossed `right - 1` and wrapped to (1, 4).
        let ground = MockGround([(3, 3), (4, 3), (5, 3)].into());
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::MemberOutside,
            &ground,
            [0],
        );
        assert_eq!(decision, Decision::MoveTo(1, 4));
    }

    #[test]
    fn no_free_cell_in_the_area_is_no_order() {
        let monster = small_squad();
        let ground = MockGround(
            (0..8)
                .flat_map(|x| (0..8).map(move |y| (x, y)))
                .collect::<HashSet<_>>(),
        );
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::MemberOutside,
            &ground,
            [0],
        );
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn a_slim_area_returns_nothing_and_does_not_panic() {
        // `rw = 0`: zero scan cells even though the coin is drawn first,
        // exactly as the original draws it. `rh = 4` sits under the jitter
        // divisor floor of two, so the coin is the only roll.
        let mut monster = roaming_at(3, 1);
        monster.spawn = k1_data::game_state::Rect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 5,
        };
        let decision = decide_monster(
            &monster,
            &standing((30, 30)),
            GroupState::MemberOutside,
            &MockGround::default(),
            [0],
        );
        assert_eq!(decision, Decision::None);
    }

    #[test]
    fn dead_or_busy_monsters_never_roll() {
        let mut dead = small_squad();
        dead.class_id = 0x80 | class::WARRIOR;
        let busy = small_squad();
        for (creature, live) in [
            (&dead, standing((30, 30))),
            (
                &busy,
                Live {
                    cell: (30, 30),
                    direction: Some(0),
                    busy: true,
                },
            ),
        ] {
            let decision = decide_monster(
                creature,
                &live,
                GroupState::MemberOutside,
                &MockGround::default(),
                [],
            );
            assert_eq!(decision, Decision::None);
        }
    }

    /// The classes the monster decision leaves alone
    /// (`game_play.cpp:3163-3164`): a strayed member of class 0x51, 0x52 or
    /// 0x53 gets no order and rolls no dice — the empty script is the proof.
    #[test]
    fn monster_classes_the_original_skips_take_no_decision() {
        for class_id in [class::WOMAN, class::CHILD, 0x53] {
            let mut creature = small_squad();
            creature.class_id = class_id;
            let decision = decide_monster(
                &creature,
                &standing((30, 30)),
                GroupState::MemberOutside,
                &MockGround::default(),
                [],
            );
            assert_eq!(decision, Decision::None, "class {class_id:#04x}");
        }
    }

    #[test]
    fn mismatched_live_and_states_lengths_do_not_panic() {
        let roster = [small_squad(), villager(class::VILLAGER, 2)];
        let live = [
            standing((30, 30)),
            standing((2, 2)),
            standing((4, 4)), // one entry more than creatures
        ];
        // No states at all: the monster reads Calm and stands; the villager
        // takes the half roll and stands.
        let decisions =
            decide_all_with_states(&roster, &live, &MockGround::default(), [49], &[], &[]);
        assert_eq!(decisions[0], Decision::None);
        assert_eq!(decisions[1], Decision::None);
        // No live entries: both decide None whatever the states say.
        let decisions = decide_all_with_states(
            &roster,
            &[],
            &MockGround::default(),
            [],
            &[],
            &[GroupState::MemberOutside, GroupState::Calm],
        );
        assert!(decisions.iter().all(|decision| *decision == Decision::None));
        // Full lengths again: the strayed squad member returns, the state
        // list one longer than the roster changes nothing.
        let decisions = decide_all_with_states(
            &roster,
            &[standing((30, 30)), standing((2, 2))],
            &MockGround::default(),
            [0, 49],
            &[],
            &[
                GroupState::MemberOutside,
                GroupState::Calm,
                GroupState::Calm,
            ],
        );
        assert_eq!(decisions[0], Decision::MoveTo(3, 3));
        assert_eq!(decisions[1], Decision::None);
    }
}
