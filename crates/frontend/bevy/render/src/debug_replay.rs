//! Optional map-session command history for the development viewer.
use std::collections::HashMap;
use std::time::Instant;

use crate::{CreatureSim, SimulationCatchUpBudget, StepKey};
use bevy::prelude::*;
use kn_game::{
    Game, MapRef, debug::DebugCommand, debug::DebugJournal, debug::DebugPlayerProfile,
    debug::DispatchOutcome, session::CreatureStage, session::CreatureState, session::Simulation,
};

#[derive(Resource, Default)]
pub struct DebugReplay {
    pub journal: DebugJournal,
    pub profile: Option<DebugPlayerProfile>,
    pub profile_error: Option<String>,
    pub map: Option<MapRef>,
    pub seed: u64,
    pub hero: Option<u32>,
    pub branch_revision: u64,
    pub last_command: Option<DispatchOutcome>,
    pub caught_up: bool,
    pub failed: Option<String>,
    pub revision: u64,
    rest_origins: HashMap<usize, u64>,
    last_steps: HashMap<usize, StepKey>,
    last_special: HashMap<usize, bool>,
    last_direction: HashMap<usize, Option<u8>>,
}

impl DebugReplay {
    pub fn start(&mut self, game: &Game, map: MapRef, seed: u64, hero: Option<u32>) {
        let profile = game.debug_player_profile(map, hero);
        *self = Self {
            map: Some(map),
            seed,
            hero,
            caught_up: true,
            profile: profile.as_ref().ok().cloned(),
            profile_error: profile.err().map(|error| error.to_string()),
            ..default()
        };
    }

    pub fn dispatch(
        &mut self,
        sim: &mut CreatureSim,
        game: &Game,
        command: DebugCommand,
    ) -> Result<(), String> {
        let map = self.map.ok_or("No map session is available")?;
        let (seed, hero) = (self.seed, self.hero);
        self.dispatch_with_factory(sim, command, || {
            game.simulation_with_hero(map, seed, hero)
                .map_err(|error| error.to_string())
        })
    }

    fn dispatch_with_factory(
        &mut self,
        sim: &mut CreatureSim,
        command: DebugCommand,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) -> Result<(), String> {
        if let Some(error) = self.failed.as_ref() {
            return Err(format!("Replay is halted: {error}"));
        }
        let simulation = sim.sim.as_mut().ok_or("Simulation is unavailable")?;
        let reset_positions = command == DebugCommand::ResetPositions;
        let reset_animation = command == DebugCommand::ResetAnimation;
        let outcome = self
            .journal
            .dispatch(simulation, self.profile.as_ref(), command, &mut fresh);
        if outcome.branched {
            self.branch_revision = self.branch_revision.wrapping_add(1);
        }
        if outcome.result.is_ok() {
            self.observe_picture(simulation, reset_positions, reset_animation);
            if reset_positions {
                sim.initial_pixels = simulation.initial_pixels();
            }
        }
        self.revision = self.revision.wrapping_add(1);
        let result = outcome.result.clone();
        self.last_command = Some(outcome);
        result
    }

    pub fn advance(
        &mut self,
        sim: &mut CreatureSim,
        game: &Game,
        target: u64,
        budget: Option<&SimulationCatchUpBudget>,
    ) {
        let Some(map) = self.map else {
            return;
        };
        let (seed, hero) = (self.seed, self.hero);
        self.advance_with_factory(sim, target, budget, || {
            game.simulation_with_hero(map, seed, hero)
                .map_err(|error| error.to_string())
        });
    }

    fn advance_with_factory(
        &mut self,
        sim: &mut CreatureSim,
        target: u64,
        budget: Option<&SimulationCatchUpBudget>,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) {
        if let Some(error) = self.failed.as_ref() {
            sim.error = Some(error.clone());
            return;
        }
        let Some(simulation) = sim.sim.as_mut() else {
            return;
        };
        if target < simulation.tick_number() {
            self.rest_origins.clear();
            self.last_steps.clear();
            self.last_special.clear();
            self.last_direction.clear();
            if let Err(error) =
                self.journal
                    .seek(simulation, self.profile.as_ref(), 0, 0, &mut fresh)
            {
                sim.sim_ticks = simulation.tick_number();
                sim.error = Some(error.clone());
                self.failed = Some(error);
                self.caught_up = false;
                return;
            }
        }
        if self.last_steps.is_empty() {
            self.observe_picture(simulation, false, false);
        }
        let started = Instant::now();
        let mut work = 0;
        loop {
            let before = if target < simulation.tick_number() {
                0
            } else {
                self.journal.applied_count()
            };
            let tick_before = simulation.tick_number();
            let progress =
                self.journal
                    .seek(simulation, self.profile.as_ref(), target, 1, &mut fresh);
            match progress {
                Ok(progress) => {
                    let applied = self.journal.applied_count();
                    let commands = &self.journal.entries()[before.min(applied)..applied];
                    let reset_positions = commands.iter().any(|entry| {
                        entry.result.as_ref().is_some_and(Result::is_ok)
                            && entry.command == DebugCommand::ResetPositions
                    });
                    let reset_animation = commands.iter().any(|entry| {
                        entry.result.as_ref().is_some_and(Result::is_ok)
                            && entry.command == DebugCommand::ResetAnimation
                    });
                    self.observe_picture(simulation, reset_positions, reset_animation);
                    sim.sim_ticks = progress.tick;
                    self.caught_up = progress.caught_up;
                    sim.error = None;
                    if tick_before != progress.tick || before != applied {
                        self.revision = self.revision.wrapping_add(1);
                    }
                    if progress.caught_up {
                        break;
                    }
                }
                Err(error) => {
                    sim.sim_ticks = simulation.tick_number();
                    self.failed = Some(error.clone());
                    self.caught_up = false;
                    sim.error = Some(error);
                    break;
                }
            }
            work += 1;
            if budget.is_some_and(|budget| {
                work >= budget.max_ticks.max(1)
                    || budget
                        .max_duration
                        .is_some_and(|limit| started.elapsed() >= limit)
            }) {
                break;
            }
        }
    }

    fn observe_picture(&mut self, sim: &Simulation, reset_positions: bool, _reset_animation: bool) {
        let tick = sim.tick_number();
        for state in sim.states() {
            let key = (state.pixel, state.step_to_pixel);
            let previous = self.last_steps.insert(state.id, key);
            let special = sim
                .creature_stage(state.id)
                .is_some_and(|stage| stage.is_special());
            let stage_changed = self
                .last_special
                .insert(state.id, special)
                .is_some_and(|previous| previous != special);
            let direction_changed = self
                .last_direction
                .insert(state.id, state.direction)
                .is_some_and(|previous| previous != state.direction);
            if !state.moving
                && !reset_positions
                && (stage_changed
                    || direction_changed
                    || previous.is_some_and(|previous| previous != key))
            {
                self.rest_origins.insert(state.id, tick);
            }
            self.rest_origins.entry(state.id).or_insert(0);
        }
    }

    pub fn animation_origin(
        &self,
        state: &CreatureState,
        stage: Option<CreatureStage>,
        logical_tick: u64,
    ) -> u64 {
        match stage {
            Some(CreatureStage::Special { frame } | CreatureStage::Melee { frame }) => {
                logical_tick.saturating_sub(u64::from(frame))
            }
            _ if state.moving => {
                logical_tick.saturating_sub(u64::from(state.step_elapsed.saturating_sub(1)))
            }
            _ => self.rest_origins.get(&state.id).copied().unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnimPlugin, AnimSet, Animator, Clip, CreatureAnim, CreatureSequences, CreatureSpawns,
        CreatureSprite, Frame, LayerSequences, PlayState, Sequence, sync_creature_animation,
    };
    use k1_data::{
        game_state::{Character, GameState, Group, Rect},
        map::KonungMap,
    };
    use kn_game::debug::JournalEntry;
    use std::sync::Arc;

    fn simulation(special_frames: u32, walking: bool) -> Simulation {
        Simulation::new(
            &KonungMap {
                ground: vec![],
                overlay: vec![],
                foot: vec![],
                decorations: vec![],
                objects: vec![],
            },
            &GameState {
                items: vec![],
                groups: vec![
                    None,
                    Some(Group {
                        slot: 1,
                        first_character: 2,
                        size: 1,
                        map_id: 1,
                        kind: if walking { 0x80 } else { 0x53 },
                        spawn: Rect {
                            left: 3,
                            top: 3,
                            right: 15,
                            bottom: 15,
                        },
                        ..default()
                    }),
                ],
                characters: vec![
                    None,
                    None,
                    Some(Character {
                        slot: 2,
                        x: 5,
                        y: 5,
                        group: 1,
                        state: if special_frames > 0 { 4 } else { 0 },
                        class_id: if walking { 0x50 } else { 0x46 },
                        ..default()
                    }),
                ],
                loot: vec![],
                exits: vec![],
                villages: vec![],
            },
            1,
            None,
            vec![[7; 8]],
            vec![special_frames],
            vec![],
            42,
            None,
        )
    }
    fn fresh() -> Result<Simulation, String> {
        Ok(simulation(12, false))
    }
    fn profile() -> DebugPlayerProfile {
        DebugPlayerProfile {
            hero: 0,
            character: 0,
            group: 0,
            direction: 4,
            class_id: 1,
            recorded_state: 0,
            step_ticks: [7; 8],
        }
    }
    fn resource() -> CreatureSim {
        CreatureSim {
            sim: Some(fresh().unwrap()),
            epoch: 77,
            ..default()
        }
    }

    #[test]
    fn relative_steps_cross_position_reset_and_restore_the_actor_from_history() {
        let mut replay = DebugReplay {
            profile: Some(profile()),
            ..default()
        };
        let mut sim = resource();
        replay.advance_with_factory(&mut sim, 100, None, fresh);
        replay
            .dispatch_with_factory(&mut sim, DebugCommand::Place { cell: (10, 10) }, fresh)
            .unwrap();
        replay.advance_with_factory(&mut sim, 200, None, fresh);
        replay
            .dispatch_with_factory(&mut sim, DebugCommand::ResetPositions, fresh)
            .unwrap();
        assert_eq!(sim.epoch, 77, "a reset is not a new history origin");
        assert!(sim.sim.as_ref().unwrap().debug_player().is_none());
        replay.advance_with_factory(&mut sim, 199, None, fresh);
        assert!(sim.sim.as_ref().unwrap().debug_player().is_some());
        replay.advance_with_factory(&mut sim, 200, None, fresh);
        assert!(sim.sim.as_ref().unwrap().debug_player().is_none());
        assert_eq!(replay.journal.entries().len(), 2);
    }

    #[test]
    fn mismatch_latches_after_synchronizing_wrapper_to_the_actual_rebuilt_tick() {
        let mut replay = DebugReplay::default();
        let mut sim = resource();
        replay.advance_with_factory(&mut sim, 20, None, fresh);
        replay
            .journal
            .replace_entries(vec![JournalEntry {
                tick: 0,
                sequence: 0,
                command: DebugCommand::ResetAnimation,
                result: Some(Err("synthetic wrong expectation".into())),
            }])
            .unwrap();
        replay.advance_with_factory(&mut sim, 0, None, fresh);
        assert!(replay.failed.is_some());
        assert_eq!(sim.sim_ticks, 0);
        assert_eq!(sim.sim.as_ref().unwrap().tick_number(), 0);
        replay.advance_with_factory(&mut sim, 10, None, || {
            panic!("a latched failure must not replay again")
        });
        assert_eq!(sim.sim.as_ref().unwrap().tick_number(), 0);
    }

    #[test]
    fn paused_boundary_commands_keep_readiness_pending_until_all_are_applied() {
        let mut replay = DebugReplay::default();
        replay
            .journal
            .replace_entries(
                (0..3)
                    .map(|sequence| JournalEntry {
                        tick: 0,
                        sequence,
                        command: DebugCommand::ResetAnimation,
                        result: None,
                    })
                    .collect(),
            )
            .unwrap();
        let mut sim = resource();
        let budget = SimulationCatchUpBudget {
            max_ticks: 1,
            max_duration: None,
        };
        replay.advance_with_factory(&mut sim, 0, Some(&budget), fresh);
        assert_eq!(sim.sim_ticks, 0);
        assert!(!replay.caught_up);
        assert_eq!(replay.journal.applied_count(), 1);
        replay.advance_with_factory(&mut sim, 0, None, fresh);
        assert!(replay.caught_up);
        assert_eq!(replay.journal.applied_count(), 3);
    }

    fn sequence(frames: usize) -> Arc<Sequence> {
        Arc::new(Sequence::new(
            (0..frames)
                .map(|index| Frame::new(Handle::default(), Vec2::ONE, Vec2::new(index as f32, 0.0)))
                .collect(),
        ))
    }
    fn layer(
        rest: &Arc<Sequence>,
        walk: &Arc<Sequence>,
        special: &Arc<Sequence>,
    ) -> LayerSequences {
        let clip = |sequence: &Arc<Sequence>| {
            Some(Clip {
                sequence: Arc::clone(sequence),
                base: 0,
                corner: (0, 0),
            })
        };
        LayerSequences {
            rest: clip(rest),
            walk: (0..8).map(|_| clip(walk)).collect(),
            special: clip(special),
            melee: Vec::new(),
            armed_rest: Vec::new(),
        }
    }
    fn render_app(special_frames: u32, walking: bool, debug: bool) -> App {
        let (rest, walk, special) = (
            sequence(5),
            sequence(7),
            sequence(special_frames.max(1) as usize),
        );
        let mut animation = CreatureAnim::default();
        animation.creatures.insert(
            2,
            CreatureSequences {
                body: [(0, layer(&rest, &walk, &special))].into(),
                shadow: layer(&rest, &walk, &special),
            },
        );
        let mut spawns = CreatureSpawns::default();
        spawns.origins.insert(2, (0, 0));
        spawns.bases.insert(2, (0, 0));
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AnimPlugin::new(60.0)))
            .insert_resource(CreatureSim {
                sim: Some(simulation(special_frames, walking)),
                ..default()
            })
            .insert_resource(animation)
            .insert_resource(spawns)
            .add_systems(Update, sync_creature_animation.before(AnimSet::Sprites));
        app.world_mut()
            .resource_mut::<crate::Clock>()
            .goto(PlayState::Paused);
        if debug {
            app.init_resource::<DebugReplay>();
        }
        app.world_mut().spawn((
            CreatureSprite {
                creature_id: 2,
                layer: 0,
                is_shadow: false,
            },
            Animator::new(rest, 0, 0),
            Sprite::default(),
            Transform::default(),
            Visibility::Inherited,
        ));
        advance_app(&mut app, 0, special_frames, walking);
        app
    }
    fn advance_app(app: &mut App, target: u64, special_frames: u32, walking: bool) {
        if app.world().contains_resource::<DebugReplay>() {
            app.world_mut()
                .resource_scope(|world, mut replay: Mut<DebugReplay>| {
                    world.resource_scope(|_, mut sim: Mut<CreatureSim>| {
                        replay.advance_with_factory(&mut sim, target, None, || {
                            Ok(simulation(special_frames, walking))
                        })
                    })
                });
        } else {
            let mut sim = app.world_mut().resource_mut::<CreatureSim>();
            while sim.sim.as_ref().unwrap().tick_number() < target {
                sim.sim.as_mut().unwrap().tick();
                sim.sim_ticks += 1;
            }
        }
        app.world_mut().resource_mut::<crate::Clock>().ticks = target;
        app.update();
    }
    fn frame(app: &mut App) -> (usize, usize, Vec3) {
        let mut query = app.world_mut().query::<(&Animator, &Transform)>();
        let (animator, transform) = query.single(app.world()).unwrap();
        (
            animator.sequence.frames.len(),
            animator.shown().unwrap(),
            transform.translation,
        )
    }
    fn dispatch_app(app: &mut App, command: DebugCommand, special_frames: u32, walking: bool) {
        app.world_mut()
            .resource_scope(|world, mut replay: Mut<DebugReplay>| {
                world.resource_scope(|_, mut sim: Mut<CreatureSim>| {
                    replay
                        .dispatch_with_factory(&mut sim, command, || {
                            Ok(simulation(special_frames, walking))
                        })
                        .unwrap()
                })
            });
        app.update();
    }

    #[test]
    fn no_command_animation_matches_existing_pipeline_and_rewind_including_tick_one_transition() {
        for duration in [1, 12] {
            let mut baseline = render_app(duration, false, false);
            let mut replay = render_app(duration, false, true);
            for tick in 0..24 {
                advance_app(&mut baseline, tick, duration, false);
                advance_app(&mut replay, tick, duration, false);
                assert_eq!(
                    frame(&mut replay),
                    frame(&mut baseline),
                    "duration={duration}, tick={tick}"
                );
            }
            let expected = frame(&mut replay);
            advance_app(&mut replay, 0, duration, false);
            advance_app(&mut replay, 23, duration, false);
            assert_eq!(frame(&mut replay), expected, "duration={duration}");
        }
    }

    #[test]
    fn special_reset_frame_and_origin_replay_through_a_position_reset() {
        let mut app = render_app(12, false, true);
        advance_app(&mut app, 5, 12, false);
        dispatch_app(&mut app, DebugCommand::ResetAnimation, 12, false);
        advance_app(&mut app, 8, 12, false);
        dispatch_app(&mut app, DebugCommand::ResetPositions, 12, false);
        advance_app(&mut app, 10, 12, false);
        let expected = frame(&mut app);
        assert_eq!(
            expected.1, 5,
            "position reset preserves the running special's origin"
        );
        advance_app(&mut app, 0, 12, false);
        advance_app(&mut app, 10, 12, false);
        assert_eq!(frame(&mut app), expected);
    }

    #[test]
    fn resetting_specials_does_not_restart_an_ordinary_non_base_walk_frame() {
        let mut app = render_app(0, true, true);
        let tick = (1..1000)
            .find(|&tick| {
                advance_app(&mut app, tick, 0, true);
                let state = app
                    .world()
                    .resource::<CreatureSim>()
                    .sim
                    .as_ref()
                    .unwrap()
                    .creature_state(0)
                    .unwrap();
                state.moving && state.step_elapsed > 1 && frame(&mut app).1 > 0
            })
            .expect("the synthetic villager must walk away from its base frame");
        let expected = frame(&mut app);
        dispatch_app(&mut app, DebugCommand::ResetAnimation, 0, true);
        assert_eq!(frame(&mut app), expected);
        advance_app(&mut app, 0, 0, true);
        advance_app(&mut app, tick, 0, true);
        assert_eq!(frame(&mut app), expected);
    }
}
