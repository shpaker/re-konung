//! Starting-state selection and construction of runtime sessions.

use k1_data::{
    game_state::{self, GameState},
    map,
};

use crate::{Game, GameError, MapRef, MapSource, Result, debug};

use k1_data::{executable::Texts, map::KonungMap};
use k1_game::{creature_anim, creatures::Workplace};
use kn_isometric::{Cell, DIRECTION_COUNT};
use kn_movement::{Event, MovementMode, WorldError};
use std::collections::HashMap;

pub use crate::inspection::{CreatureInspection, DebugPursuit, PursuitError, PursuitStatus};
pub use k1_game::simulation::{
    AttackError, AttackOrder, AttackStatus, CreatureStage, CreatureState, GoalReach, GroupState,
    HeroOrder, HeroOrderError, HeroOrderStatus, MapPassage, WalkState,
};

impl Game {
    /// Drops the session overlay when leaving or retrying a session.
    pub fn clear_new_game(&mut self) {
        self.session_state = None;
    }

    pub(crate) fn session_or_original(
        &self,
        bytes: &[u8],
        source: MapSource,
        hero: Option<usize>,
    ) -> Result<GameState> {
        if source == MapSource::Game
            && let Some((chosen, state)) = &self.session_state
            && hero == Some(*chosen as usize)
        {
            return Ok(state.clone());
        }
        Ok(game_state::parse(bytes)?)
    }

    /// The single place a creature/character subsystem decides which `Game.N` to
    /// load: the caller's `requested` hero, else the first available. The Scene,
    /// the simulation and the clips all ask here, so choosing a hero other than
    /// the first never splits them across different starting states — geometry,
    /// roster and clips stay on one `Game.N` (review #101, finding P2-2).
    pub(crate) fn hero_for(
        &self,
        source: MapSource,
        requested: Option<u32>,
    ) -> Result<Option<usize>> {
        Ok(match requested {
            Some(hero) => Some(hero as usize),
            None => self.heroes(source)?.first().map(|&h| h as usize),
        })
    }

    /// Карта, на которой начинается новая игра героя.
    pub fn start_map(&self, hero: u32) -> MapRef {
        MapRef {
            source: MapSource::Game,
            number: game_state::start_map(hero as usize) as u32,
        }
    }

    /// Headless simulation of one map using the first available hero's state:
    /// creatures walk, rules decide, engine ticks.
    pub fn simulation(&self, map_ref: MapRef, seed: u64) -> Result<Simulation> {
        self.simulation_with_hero(map_ref, seed, None)
    }

    /// The same, for a chosen hero: `hero` selects the `Game.N` whose state and
    /// roster drive it, `None` the first. A caller that composed the Scene for a
    /// specific hero must pass the same one here, or the creature geometry, the
    /// roster and the clips come from different files (review #101, finding P2-2).
    pub fn simulation_with_hero(
        &self,
        map_ref: MapRef,
        seed: u64,
        hero: Option<u32>,
    ) -> Result<Simulation> {
        let files = map_ref.source.file_set();
        let map_bytes = self.dir.map_bytes(map_ref.number, files)?;
        let konung_map = map::parse(&map_bytes)?;
        let hero = self.hero_for(map_ref.source, hero)?;
        let state_bytes = match hero {
            Some(hero) => Some(self.dir.state_bytes(hero as u32, files)?),
            None => None,
        };
        let state = state_bytes
            .as_ref()
            .map(|bytes| self.session_or_original(bytes, map_ref.source, hero))
            .transpose()?;
        let state = state
            .ok_or_else(|| GameError::Io(std::io::Error::other("no game state for simulation")))?;
        let jobs = k1_game::creatures::job_slots(&state, map_ref.number as usize);
        let (step_table, special_frames, melee_frames) =
            self.resources.with_graphics(|graph, objects| {
                let roster = k1_game::creatures::roster(&state, map_ref.number as usize, hero);
                let steps = k1_game::creatures::step_durations(&state, &roster, graph, objects);
                let specials =
                    k1_game::creatures::special_frame_counts(&state, &roster, graph, objects);
                let melee = k1_game::creatures::melee_frame_counts(&state, &roster, graph, objects);
                (steps, specials, melee)
            })?;
        let mut simulation = Simulation::new(
            &konung_map,
            &state,
            map_ref.number as usize,
            hero,
            step_table,
            special_frames,
            jobs,
            seed,
            self.resources.tables.texts.as_ref(),
        );
        simulation.configure_melee_frames(melee_frames);
        Ok(simulation)
    }

    /// Resolves the same source/hero as a map session, but takes the actual
    /// player leader's pace from that hero's start map, not the inspected map.
    pub fn debug_player_profile(
        &self,
        map_ref: MapRef,
        hero: Option<u32>,
    ) -> Result<debug::DebugPlayerProfile> {
        let files = map_ref.source.file_set();
        let hero = self.hero_for(map_ref.source, hero)?.ok_or_else(|| {
            GameError::Io(std::io::Error::other("no hero state for debug player"))
        })?;
        let bytes = self.dir.state_bytes(hero as u32, files)?;
        let state = self.session_or_original(&bytes, map_ref.source, Some(hero))?;
        let roster = k1_game::creatures::roster(&state, game_state::start_map(hero), Some(hero));
        let candidates: Vec<_> = roster
            .into_iter()
            .filter(|creature| creature.group_kind == k1_game::rules::group::PLAYER)
            .collect();
        let steps = self.resources.with_graphics(|graph, objects| {
            k1_game::creatures::step_durations(&state, &candidates, graph, objects)
        })?;
        candidates
            .into_iter()
            .zip(steps)
            .map(|(creature, step_ticks)| debug::DebugPlayerProfile {
                hero: hero as u32,
                character: creature.id,
                group: creature.group,
                direction: creature.direction,
                class_id: creature.class_id,
                recorded_state: creature.state,
                step_ticks,
            })
            .find(|profile| profile.validate().is_ok())
            .ok_or_else(|| {
                GameError::Io(std::io::Error::other(
                    "no living player member with complete walk data",
                ))
            })
    }
}

/// A concrete K1 session behind the series facade.
pub struct Simulation {
    inner: k1_game::simulation::Simulation,
}

impl Simulation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        map: &KonungMap,
        state: &GameState,
        map_number: usize,
        hero: Option<usize>,
        step_table: Vec<[u32; DIRECTION_COUNT]>,
        special_frames: Vec<u32>,
        jobs: Vec<Workplace>,
        seed: u64,
        texts: Option<&Texts>,
    ) -> Simulation {
        Self {
            inner: k1_game::simulation::Simulation::new(
                map,
                state,
                map_number,
                hero,
                step_table,
                special_frames,
                jobs,
                seed,
                texts,
            ),
        }
    }

    pub fn hero_id(&self) -> Option<usize> {
        self.inner.hero_id()
    }

    pub fn hero_order(&self) -> Option<HeroOrder> {
        self.inner.hero_order()
    }

    pub fn hero_goal_reaches(&self) -> u64 {
        self.inner.hero_goal_reaches()
    }

    pub fn order_hero_move(&mut self, target: Cell) -> std::result::Result<usize, HeroOrderError> {
        self.inner.order_hero_move(target)
    }

    pub fn order_hero_move_with_mode(
        &mut self,
        target: Cell,
        mode: MovementMode,
    ) -> std::result::Result<usize, HeroOrderError> {
        self.inner.order_hero_move_with_mode(target, mode)
    }

    pub fn spawn_failures(&self) -> &[(usize, WorldError)] {
        self.inner.spawn_failures()
    }

    pub fn order_failures(&self) -> &[(usize, WorldError)] {
        self.inner.order_failures()
    }

    pub fn load_trimmed(&self) -> &[usize] {
        self.inner.load_trimmed()
    }

    pub fn goal_reaches(&self) -> Vec<GoalReach> {
        self.inner.goal_reaches()
    }

    pub fn tick_number(&self) -> u64 {
        self.inner.tick_number()
    }

    pub fn game_time(&self) -> k1_game::rules::time_of_day::GameTime {
        self.inner.game_time()
    }

    pub fn creature_count(&self) -> usize {
        self.inner.creature_count()
    }

    pub fn initial_pixels(&self) -> Vec<(usize, (i32, i32))> {
        self.inner.initial_pixels()
    }

    pub fn adopt_positions_from(&mut self, fresh: Simulation) {
        self.inner.adopt_positions_from(fresh.inner)
    }

    pub fn tick(&mut self) -> Vec<Event> {
        self.inner.tick()
    }

    pub fn states(&self) -> Vec<CreatureState> {
        self.inner.states()
    }

    pub fn labels(&self) -> &HashMap<usize, String> {
        self.inner.labels()
    }

    pub fn creature_name(&self, creature_id: usize) -> Option<&str> {
        self.inner.creature_name(creature_id)
    }

    pub fn creature_stage(&self, creature_id: usize) -> Option<creature_anim::Stage> {
        self.inner.creature_stage(creature_id)
    }

    pub fn creature_shown_state(&self, creature_id: usize) -> Option<u8> {
        self.inner.creature_shown_state(creature_id)
    }

    pub fn restart_creature_special(&mut self, creature_id: usize) -> bool {
        self.inner.restart_creature_special(creature_id)
    }

    pub fn restart_all_specials(&mut self) {
        self.inner.restart_all_specials()
    }

    pub fn pose_rebuilds(&self) -> u64 {
        self.inner.pose_rebuilds()
    }

    pub fn drain_creature_anim_changes(&mut self) -> Vec<usize> {
        self.inner.drain_creature_anim_changes()
    }

    pub fn creature_state(&self, index: usize) -> Option<CreatureState> {
        self.inner.creature_state(index)
    }

    pub fn configure_melee_frames(&mut self, counts: Vec<[u32; DIRECTION_COUNT]>) {
        self.inner.configure_melee_frames(counts)
    }

    pub fn attack_order(&self, creature_id: usize) -> Option<AttackOrder> {
        self.inner.attack_order(creature_id)
    }

    pub fn hero_attack_order(&self) -> Option<AttackOrder> {
        self.inner.hero_attack_order()
    }

    pub fn can_hero_attack(&self, target_id: usize) -> bool {
        self.inner.can_hero_attack(target_id)
    }

    pub fn order_hero_attack(
        &mut self,
        target_id: usize,
    ) -> std::result::Result<usize, AttackError> {
        self.inner.order_hero_attack(target_id)
    }

    pub fn debug_pursuit(&self, creature_id: usize) -> Option<DebugPursuit> {
        self.inner.debug_pursuit(creature_id).map(Into::into)
    }

    pub fn inspect_creature(&self, id: usize) -> Option<CreatureInspection> {
        self.inner.inspect_creature(id).map(Into::into)
    }

    pub fn inspect_creatures(&self) -> Vec<CreatureInspection> {
        self.inner
            .inspect_creatures()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn apply_debug_command(
        &mut self,
        command: &debug::DebugCommand,
        profile: Option<&debug::DebugPlayerProfile>,
        mut fresh: impl FnMut() -> std::result::Result<Simulation, String>,
    ) -> std::result::Result<(), String> {
        match command {
            debug::DebugCommand::Place { cell } => {
                let profile = profile.map(k1_game::simulation::ActorProfile::from);
                self.inner
                    .place_actor(Cell::new(cell.0, cell.1), profile.as_ref())
            }
            debug::DebugCommand::Move { cell } => self.inner.move_actor(Cell::new(cell.0, cell.1)),
            debug::DebugCommand::Remove => self.inner.remove_actor(),
            debug::DebugCommand::ResetPositions => self.inner.reset_positions_from(fresh()?.inner),
            debug::DebugCommand::ResetAnimation => {
                self.inner.restart_all_specials();
                Ok(())
            }
        }
    }

    pub fn debug_player(&self) -> Option<debug::DebugPlayerSnapshot> {
        self.inner.debug_player().map(Into::into)
    }

    pub fn debug_snapshot(&self) -> debug::DebugSnapshot {
        self.inner.debug_snapshot().into()
    }
}
