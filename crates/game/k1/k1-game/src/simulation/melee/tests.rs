use super::*;
use k1_data::game_state::{Character, Group, Rect};

fn fixture(class_id: u8, hero_cell: Cell) -> (Simulation, GameState) {
    let spawn = Rect {
        left: 42,
        top: 78,
        right: 44,
        bottom: 82,
    };
    let state = GameState {
        items: vec![],
        loot: vec![],
        exits: vec![],
        villages: vec![],
        groups: vec![
            Some(Group {
                slot: 0,
                first_character: 0,
                size: 1,
                kind: 0x20,
                ..Group::default()
            }),
            Some(Group {
                slot: 1,
                first_character: 1,
                size: 1,
                kind: 0x41,
                map_id: 1,
                spawn,
                leash: Rect {
                    left: 30,
                    top: 60,
                    right: 60,
                    bottom: 100,
                },
                ..Group::default()
            }),
        ],
        characters: vec![
            Some(Character {
                slot: 0,
                class_id: 5,
                group: 0,
                x: hero_cell.column as i16,
                y: hero_cell.row as i16,
                attributes: [1600; 9],
                ..Character::default()
            }),
            Some(Character {
                slot: 1,
                class_id,
                group: 1,
                attributes: [1600; 9],
                ..Character::default()
            }),
        ],
    };
    let map = KonungMap {
        ground: vec![],
        overlay: vec![],
        foot: vec![vec![0; 160]; 320],
        decorations: vec![],
        objects: vec![],
    };
    let mut simulation = Simulation::new(
        &map,
        &state,
        1,
        Some(0),
        vec![[2; 8]; 2],
        vec![0; 2],
        vec![],
        7,
        None,
    );
    simulation.configure_melee_frames(vec![[4; 8]; 2]);
    assert_eq!(
        simulation
            .world
            .state(simulation.ids[1].unwrap())
            .unwrap()
            .cell,
        Cell::new(43, 80)
    );
    (simulation, state)
}

fn advance_until(simulation: &mut Simulation, predicate: impl Fn(&Simulation) -> bool) {
    for _ in 0..300 {
        if predicate(simulation) {
            return;
        }
        simulation.tick();
    }
    assert!(
        predicate(simulation),
        "condition not reached, orders={:?}",
        simulation.melee.orders
    );
}

#[test]
fn manual_attack_approaches_turns_and_repeats_without_hit_or_death_stages() {
    let (mut simulation, _) = fixture(0x44, Cell::new(40, 80));
    simulation.order_hero_attack(1).unwrap();
    advance_until(&mut simulation, |simulation| {
        matches!(
            simulation.creature_stage(0),
            Some(CreatureStage::Melee { frame: 0 })
        )
    });
    let attacker = simulation.world.state(simulation.ids[0].unwrap()).unwrap();
    let target = simulation.world.state(simulation.ids[1].unwrap()).unwrap();
    assert_eq!(
        Direction::between(attacker.cell, target.cell),
        attacker.direction
    );
    assert!(!attacker.moving);
    assert_ne!(attacker.cell, target.cell);
    for frame in [1, 2, 3, 0, 1, 2, 3, 0] {
        simulation.tick();
        assert_eq!(
            simulation.creature_stage(0),
            Some(CreatureStage::Melee { frame })
        );
    }
    assert!(
        [0, 1]
            .into_iter()
            .all(|id| simulation.creature_shown_state(id) == Some(5))
    );
    assert_eq!(
        simulation.hero_attack_order().unwrap().status,
        AttackStatus::Attacking
    );
    assert!(simulation.order_failures().is_empty());
}

#[test]
fn monsters_detect_the_hero_and_the_idle_hero_answers() {
    let (mut simulation, _) = fixture(0x42, Cell::new(42, 80));
    advance_until(&mut simulation, |simulation| {
        simulation
            .attack_order(1)
            .is_some_and(|order| order.status == AttackStatus::Attacking)
    });
    assert_eq!(simulation.attack_order(1).unwrap().target_id, 0);
    advance_until(&mut simulation, |simulation| {
        simulation
            .hero_attack_order()
            .is_some_and(|order| order.status == AttackStatus::Attacking)
    });
    assert_eq!(simulation.hero_attack_order().unwrap().target_id, 1);
}

#[test]
fn a_move_waits_for_the_swing_then_leaves_without_auto_response_interrupting_it() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    let target = Cell::new(35, 80);
    simulation.order_hero_move(target).unwrap();
    for frame in 1..4 {
        simulation.tick();
        assert_eq!(
            simulation.creature_stage(0),
            Some(CreatureStage::Melee { frame })
        );
        assert_eq!(
            simulation.hero_order().unwrap().status,
            HeroOrderStatus::Pending
        );
    }
    simulation.tick();
    assert_eq!(simulation.creature_stage(0), Some(CreatureStage::Idle));
    advance_until(&mut simulation, |simulation| {
        simulation.hero_order().unwrap().status == HeroOrderStatus::Arrived
    });
    assert_eq!(
        simulation
            .world
            .state(simulation.ids[0].unwrap())
            .unwrap()
            .cell,
        target
    );
    assert_eq!(simulation.hero_goal_reaches(), 1);
    assert!(simulation.hero_attack_order().is_none());
}

#[test]
fn the_latest_queued_order_wins_and_an_invalid_target_does_not_replace_it() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    simulation.order_hero_move(Cell::new(35, 80)).unwrap();
    simulation.order_hero_attack(1).unwrap();
    assert_eq!(
        simulation.order_hero_attack(999),
        Err(AttackError::TargetUnavailable)
    );
    for _ in 0..5 {
        simulation.tick();
    }
    assert_eq!(
        simulation.hero_attack_order().unwrap().status,
        AttackStatus::Attacking
    );
    assert!(simulation.hero_order().is_none());
    assert!(
        !simulation
            .world
            .state(simulation.ids[0].unwrap())
            .unwrap()
            .moving
    );
}

#[test]
fn absent_frames_and_friendly_targets_are_explicit_refusals() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.configure_melee_frames(vec![[0; 8]; 2]);
    assert!(simulation.can_hero_attack(1));
    assert_eq!(
        simulation.order_hero_attack(1),
        Err(AttackError::MissingAnimation)
    );
    assert_eq!(
        simulation.order_hero_attack(0),
        Err(AttackError::InvalidTarget)
    );
    simulation.roster[1].group_kind = 0x80;
    assert!(!simulation.can_hero_attack(1));
    assert_eq!(
        simulation.order_hero_attack(1),
        Err(AttackError::InvalidTarget)
    );
}

#[test]
fn special_animation_finishes_before_an_attack_can_start() {
    let (mut simulation, _) = fixture(0x46, Cell::new(42, 80));
    simulation.creature_anim.enter_world([
        CreatureAnimSeed {
            id: 0,
            recorded_state: 0,
            special_frames: 0,
        },
        CreatureAnimSeed {
            id: 1,
            recorded_state: 4,
            special_frames: 6,
        },
    ]);
    for frame in 1..6 {
        simulation.tick();
        assert_eq!(
            simulation.creature_stage(1),
            Some(CreatureStage::Special { frame })
        );
        assert_eq!(simulation.attack_order(1), None);
    }
    simulation.tick();
    assert_eq!(
        simulation.creature_stage(1),
        Some(CreatureStage::Melee { frame: 0 })
    );
}

#[test]
fn removal_of_a_target_cancels_the_swing() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    simulation.world.remove(simulation.ids[1].unwrap()).unwrap();
    simulation.tick();
    assert_eq!(simulation.creature_stage(0), Some(CreatureStage::Idle));
    assert_eq!(
        simulation.hero_attack_order().unwrap().status,
        AttackStatus::Refused(AttackError::TargetUnavailable)
    );
}

#[test]
fn unreachable_targets_report_once_and_retries_are_throttled() {
    let (mut simulation, _) = fixture(0x44, Cell::new(40, 80));
    for row in 0..FINE_HEIGHT {
        simulation.world.passage_mut().blocked.insert((41, row));
    }
    simulation.order_hero_attack(1).unwrap();
    let events = simulation.tick();
    assert_eq!(
        simulation.hero_attack_order().unwrap().status,
        AttackStatus::Refused(AttackError::NoRoute)
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::NoRoute { .. }))
            .count(),
        1
    );
    for _ in 0..20 {
        simulation.order_hero_attack(1).unwrap();
        assert!(
            !simulation
                .tick()
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. } | Event::NoRoute { .. }))
        );
    }
}

#[test]
fn reset_positions_clears_combat_and_keeps_specials_separate() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    simulation.order_hero_move(Cell::new(35, 80)).unwrap();
    let (fresh, _) = fixture(0x44, Cell::new(42, 80));
    let tick = simulation.tick;
    simulation.adopt_positions_from(fresh);
    assert_eq!(simulation.tick, tick);
    assert!(simulation.melee.orders.iter().all(Option::is_none));
    assert!(simulation.hero_attack_order().is_none());
    assert_eq!(simulation.creature_stage(0), Some(CreatureStage::Idle));
}

#[test]
fn identical_commands_replay_attack_frames_and_movement_exactly() {
    let (mut first, _) = fixture(0x42, Cell::new(40, 80));
    let (mut second, _) = fixture(0x42, Cell::new(40, 80));
    for tick in 0..120 {
        for simulation in [&mut first, &mut second] {
            match tick {
                0 | 60 => {
                    simulation.order_hero_attack(1).unwrap();
                }
                30 | 90 => {
                    simulation.order_hero_move(Cell::new(35, 80)).unwrap();
                }
                _ => {}
            }
        }
        assert_eq!(first.tick(), second.tick());
        assert_eq!(first.melee.orders, second.melee.orders);
        for id in [0, 1] {
            assert_eq!(first.creature_stage(id), second.creature_stage(id));
        }
        assert_eq!(first.debug_snapshot(), second.debug_snapshot());
    }
}

#[test]
fn retreating_class_walks_after_a_completed_swing() {
    let (mut simulation, _) = fixture(0x49, Cell::new(42, 80));
    advance_until(&mut simulation, |simulation| {
        matches!(
            simulation.creature_stage(1),
            Some(CreatureStage::Melee { frame: 0 })
        )
    });
    for _ in 0..4 {
        simulation.tick();
    }
    assert!(simulation.melee.retreating[1]);
    let state = simulation.world.state(simulation.ids[1].unwrap()).unwrap();
    assert!(state.moving);
    assert_eq!(simulation.creature_stage(1), Some(CreatureStage::Idle));
}

#[test]
fn attack_routes_never_accept_a_second_ring_or_the_targets_cell() {
    let (mut simulation, _) = fixture(0x44, Cell::new(40, 80));
    let target = Cell::new(43, 80);
    for direction in Direction::ALL {
        let cell = target.neighbour(direction).unwrap();
        simulation
            .world
            .passage_mut()
            .blocked
            .insert((cell.column, cell.row));
    }
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    assert_eq!(
        simulation.hero_attack_order().unwrap().status,
        AttackStatus::Refused(AttackError::NoRoute)
    );
    assert_eq!(
        simulation
            .world
            .state(simulation.ids[0].unwrap())
            .unwrap()
            .cell,
        Cell::new(40, 80)
    );
}

fn add_enemy(simulation: &mut Simulation, cell: Cell) -> usize {
    let id = simulation.roster.len();
    let mut creature = simulation.roster[1];
    creature.id = id;
    creature.cell = (cell.column as i16, cell.row as i16);
    let world_id = simulation.world.add(cell).unwrap();
    simulation.roster.push(creature);
    simulation.ids.push(Some(world_id));
    simulation.by_creature.push(id);
    simulation.melee.orders.push(None);
    simulation.melee.pursuits.push(None);
    simulation.melee.target_cells.push(None);
    simulation.melee.retry_at.push(0);
    simulation.melee.retreating.push(false);
    simulation.melee.leashes.push(simulation.melee.leashes[1]);
    simulation.melee.frames.push([4; 8]);
    simulation
        .creature_anim
        .enter_world(simulation.roster.iter().map(|creature| CreatureAnimSeed {
            id: creature.id,
            recorded_state: 0,
            special_frames: 0,
        }));
    id
}

#[test]
fn a_new_target_does_not_inherit_a_failed_targets_cooldown() {
    let (mut simulation, _) = fixture(0x44, Cell::new(40, 80));
    let second = add_enemy(&mut simulation, Cell::new(39, 80));
    for row in 0..FINE_HEIGHT {
        simulation.world.passage_mut().blocked.insert((41, row));
    }
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    assert!(simulation.melee.retry_at[0] > simulation.tick);
    simulation.order_hero_attack(second).unwrap();
    simulation.tick();
    assert_eq!(
        simulation.hero_attack_order(),
        Some(AttackOrder {
            target_id: second,
            status: AttackStatus::Attacking
        })
    );
}

#[test]
fn changing_targets_mid_swing_keeps_the_old_swing_until_its_last_frame() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    let second = add_enemy(&mut simulation, Cell::new(41, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    simulation.order_hero_attack(second).unwrap();
    for frame in 1..4 {
        simulation.tick();
        assert_eq!(
            simulation.creature_stage(0),
            Some(CreatureStage::Melee { frame })
        );
        assert_eq!(simulation.attack_order(0).unwrap().target_id, 1);
        assert_eq!(
            simulation.hero_attack_order().unwrap().status,
            AttackStatus::Pending
        );
    }
    simulation.tick();
    assert_eq!(simulation.attack_order(0).unwrap().target_id, second);
    assert_eq!(
        simulation
            .world
            .state(simulation.ids[0].unwrap())
            .unwrap()
            .direction,
        Some(Direction::West)
    );
}

#[test]
fn pursuit_tracks_a_moving_target_before_the_old_route_finishes() {
    let (mut simulation, _) = fixture(0x42, Cell::new(35, 80));
    simulation.roster[1].spawn = Rect {
        left: 10,
        top: 60,
        right: 60,
        bottom: 100,
    };
    simulation.melee.leashes[1] = simulation.roster[1].spawn;
    simulation.order_hero_move(Cell::new(15, 80)).unwrap();
    let mut corrections = 0;
    for _ in 0..30 {
        let before = simulation
            .world
            .state(simulation.ids[0].unwrap())
            .unwrap()
            .cell;
        let old = simulation.melee.target_cells[1];
        simulation.tick();
        if old.is_some_and(|cell| cell != before) {
            let movement = simulation
                .world
                .debug_state(simulation.ids[1].unwrap())
                .unwrap();
            assert_eq!(movement.requested_target, Some(before));
            assert_eq!(simulation.melee.target_cells[1], Some(before));
            corrections += 1;
        }
    }
    assert!(corrections > 3);
}

#[test]
fn a_moving_target_crossing_the_leash_ends_pursuit() {
    let (mut simulation, _) = fixture(0x42, Cell::new(35, 80));
    simulation.roster[1].spawn = Rect {
        left: 10,
        top: 60,
        right: 60,
        bottom: 100,
    };
    simulation.order_hero_move(Cell::new(25, 80)).unwrap();
    advance_until(&mut simulation, |simulation| {
        simulation
            .attack_order(1)
            .is_some_and(|order| order.status == AttackStatus::Approaching)
    });
    advance_until(&mut simulation, |simulation| {
        simulation
            .attack_order(1)
            .is_some_and(|order| order.status == AttackStatus::Finished)
    });
    let hero = simulation.world.state(simulation.ids[0].unwrap()).unwrap();
    assert!(hero.cell.column < 30);
    assert!(
        !simulation
            .creature_stage(1)
            .is_some_and(|stage| matches!(stage, CreatureStage::Melee { .. }))
    );
}

#[test]
fn multiple_attackers_keep_exclusive_cells_and_answer_the_same_hero() {
    let (mut simulation, _) = fixture(0x42, Cell::new(42, 80));
    add_enemy(&mut simulation, Cell::new(44, 80));
    add_enemy(&mut simulation, Cell::new(43, 82));
    for _ in 0..100 {
        simulation.tick();
        let cells: HashSet<_> = simulation.states().iter().map(|state| state.cell).collect();
        assert_eq!(cells.len(), 4);
    }
    for id in 1..4 {
        assert_eq!(simulation.attack_order(id).unwrap().target_id, 0);
        assert_eq!(
            simulation.attack_order(id).unwrap().status,
            AttackStatus::Attacking
        );
    }
}

#[test]
fn invalid_move_during_a_swing_preserves_the_attack() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    assert_eq!(
        simulation.order_hero_move(Cell::new(FINE_WIDTH, 0)),
        Err(HeroOrderError::Movement(WorldError::OutsideGrid))
    );
    for _ in 0..8 {
        simulation.tick();
    }
    assert_eq!(
        simulation.attack_order(0).unwrap().status,
        AttackStatus::Attacking
    );
}

#[test]
fn frame_two_does_not_provoke_a_squad_after_its_target_disappears() {
    let (mut simulation, _) = fixture(0x44, Cell::new(42, 80));
    simulation.roster[1].spawn = Rect {
        left: 43,
        right: 43,
        top: 80,
        bottom: 80,
    };
    simulation.order_hero_attack(1).unwrap();
    simulation.tick();
    simulation.tick();
    assert_eq!(
        simulation.creature_stage(0),
        Some(CreatureStage::Melee { frame: 1 })
    );
    simulation.world.remove(simulation.ids[1].unwrap()).unwrap();
    simulation.tick();
    assert_eq!(simulation.group_states[1], GroupState::Calm);
}

#[test]
fn manual_attack_follows_a_retreating_enemy_but_auto_response_does_not() {
    for manual in [false, true] {
        let (mut simulation, _) = fixture(0x49, Cell::new(42, 80));
        if manual {
            simulation.order_hero_attack(1).unwrap();
        }
        advance_until(&mut simulation, |simulation| simulation.melee.retreating[1]);
        for _ in 0..10 {
            simulation.tick();
        }
        let order = simulation.attack_order(0).unwrap();
        if manual {
            assert!(order.status.active());
            assert!(
                simulation
                    .world
                    .state(simulation.ids[0].unwrap())
                    .unwrap()
                    .moving
            );
        } else {
            assert_eq!(order.status, AttackStatus::Finished);
            assert!(
                !simulation
                    .world
                    .state(simulation.ids[0].unwrap())
                    .unwrap()
                    .moving
            );
        }
    }
}

#[test]
fn a_viewer_marker_does_not_send_monsters_after_a_distant_real_hero() {
    let (mut simulation, _) = fixture(0x42, Cell::new(10, 80));
    let profile = ActorProfile {
        hero: 0,
        character: 0,
        group: 0,
        direction: 0,
        class_id: 5,
        recorded_state: 0,
        step_ticks: [2; 8],
    };
    simulation
        .place_actor(Cell::new(42, 80), Some(&profile))
        .unwrap();
    for _ in 0..10 {
        simulation.tick();
    }
    assert_eq!(simulation.group_states[1], GroupState::PlayerInside);
    assert_eq!(simulation.attack_order(1), None);
}

fn debug_profile() -> ActorProfile {
    ActorProfile {
        hero: 0,
        character: 0,
        group: 0,
        direction: 0,
        class_id: 5,
        recorded_state: 0,
        step_ticks: [1; 8],
    }
}

fn place_debug(simulation: &mut Simulation, cell: (u32, u32)) {
    simulation
        .place_actor(Cell::new(cell.0, cell.1), Some(&debug_profile()))
        .unwrap();
}

fn debug_encounter() -> Simulation {
    let (mut simulation, _) = fixture(0x42, Cell::new(10, 80));
    simulation.roster[1].spawn.left = 30;
    simulation.configure_melee_frames(vec![[0; 8]; 2]);
    place_debug(&mut simulation, (35, 80));
    simulation
}

#[test]
fn debug_pursuit_reaches_a_free_neighbour_without_an_attack_contract() {
    let mut simulation = debug_encounter();
    simulation.tick();
    assert_eq!(simulation.group_states[1], GroupState::PlayerInside);
    assert_eq!(
        simulation.debug_pursuit(1).unwrap().status,
        PursuitStatus::Approaching
    );
    assert_eq!(simulation.debug_pursuit(1).unwrap().target_id, 0);
    advance_until(&mut simulation, |simulation| {
        simulation.debug_pursuit(1).unwrap().status == PursuitStatus::InRange
    });
    for _ in 0..40 {
        simulation.tick();
        let creature = simulation.world.state(simulation.ids[1].unwrap()).unwrap();
        let player = simulation.debug_player().unwrap().movement.state;
        assert!(Direction::between(creature.cell, player.cell).is_some());
        assert!(!creature.moving);
        assert_eq!(simulation.attack_order(1), None);
        assert_eq!(simulation.creature_stage(1), Some(CreatureStage::Idle));
        assert_eq!(
            simulation.inspect_creature(1).unwrap().pursuit,
            simulation.debug_pursuit(1)
        );
    }
}

#[test]
fn calm_groups_do_not_acquire_a_distant_debug_target() {
    let (mut simulation, _) = fixture(0x42, Cell::new(10, 80));
    place_debug(&mut simulation, (35, 80));
    for _ in 0..40 {
        assert!(simulation.tick().is_empty());
        assert_eq!(simulation.debug_pursuit(1), None);
        assert_eq!(simulation.group_states[1], GroupState::Calm);
    }
}

#[test]
fn debug_pursuit_retargets_then_abandons_a_moved_target_outside_the_leash() {
    // Retention: original 0x41212A/0x41213A, rectangle 0x412170..0x4121A4.
    let mut simulation = debug_encounter();
    simulation.tick();
    simulation.move_actor(Cell::new(25, 80)).unwrap();
    let mut changes = 0;
    for _ in 0..40 {
        let cell = simulation.debug_player().unwrap().movement.state.cell;
        let target = (cell.column, cell.row);
        let old = simulation.melee.target_cells[1];
        simulation.tick();
        let pursuit = simulation.debug_pursuit(1).unwrap();
        if pursuit.status == PursuitStatus::Finished {
            assert!(target.0 < 30);
            assert!(changes > 0);
            return;
        }
        if old.is_some_and(|cell| (cell.column, cell.row) != target) {
            assert_eq!(pursuit.target_cell, target);
            changes += 1;
        }
    }
    panic!("target never crossed the retention boundary");
}

#[test]
fn debug_target_removal_finishes_the_step_then_returns_without_reusing_identity() {
    let mut simulation = debug_encounter();
    simulation.tick();
    let old_target = simulation.debug_pursuit(1).unwrap().target_id;
    let before = simulation
        .world
        .debug_state(simulation.ids[1].unwrap())
        .unwrap();
    simulation.remove_actor().unwrap();
    let after = simulation
        .world
        .debug_state(simulation.ids[1].unwrap())
        .unwrap();
    assert_eq!(
        before, after,
        "removal does not mutate another actor before a tick"
    );
    simulation.tick();
    assert_eq!(
        simulation.debug_pursuit(1).unwrap().status,
        PursuitStatus::Refused(PursuitError::TargetUnavailable)
    );
    for _ in 0..100 {
        simulation.tick();
    }
    let actor = simulation.world.state(simulation.ids[1].unwrap()).unwrap();
    assert!(inside_rectangle(simulation.roster[1].spawn, actor.cell));
    assert!(!actor.moving);
    place_debug(&mut simulation, (35, 80));
    simulation.tick();
    assert_ne!(simulation.debug_player().unwrap().id, old_target);
    assert_eq!(
        simulation.debug_pursuit(1).unwrap().target_id,
        simulation.debug_player().unwrap().id
    );
}

#[test]
fn a_live_hero_and_marker_share_the_existing_target_ranking() {
    let (mut simulation, _) = fixture(0x42, Cell::new(42, 80));
    place_debug(&mut simulation, (44, 80));
    simulation.tick();
    assert_eq!(simulation.attack_order(1).unwrap().target_id, 0);
    assert_eq!(simulation.debug_pursuit(1), None);
    let mut simulation = debug_encounter();
    simulation.tick();
    assert!(simulation.debug_pursuit(1).is_some());
    assert_eq!(simulation.attack_order(1), None);
}

#[test]
fn failed_debug_routes_have_one_refusal_per_cooldown() {
    let mut simulation = debug_encounter();
    for row in 0..FINE_HEIGHT {
        simulation.world.passage_mut().blocked.insert((41, row));
    }
    let events = simulation.tick();
    assert_eq!(
        simulation.debug_pursuit(1).unwrap().status,
        PursuitStatus::Refused(PursuitError::NoRoute)
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::NoRoute { .. }))
            .count(),
        1
    );
    for _ in 0..20 {
        assert!(
            !simulation
                .tick()
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. } | Event::NoRoute { .. }))
        );
    }
    assert!(simulation.order_failures().is_empty());
}

#[test]
fn target_death_during_a_chase_abandons_the_old_route() {
    // Target filtering: original selectors 0x420098 (RUS) / 0x42011C (ENG).
    for death_state in [None, Some(3), Some(9)] {
        let (mut simulation, _) = fixture(0x42, Cell::new(35, 80));
        simulation.roster[1].spawn.left = 30;
        let id = simulation.ids[1].unwrap();
        let events = simulation.tick();
        assert!(events.iter().any(|event| matches!(event,
            Event::RouteSet { creature, .. } if *creature == id)));
        assert_eq!(
            simulation.attack_order(1).unwrap().status,
            AttackStatus::Approaching
        );
        let before = simulation.world.state(id).unwrap();
        let next = before.step_to.unwrap();
        if let Some(state) = death_state {
            simulation.roster[0].state = state;
        } else {
            simulation.roster[0].class_id |= 0x80;
        }
        assert!(
            simulation.tick().is_empty(),
            "in-flight step continues without a new route"
        );
        assert_eq!(
            simulation.attack_order(1).unwrap().status,
            AttackStatus::Refused(AttackError::TargetUnavailable)
        );
        assert_eq!(simulation.world.state(id).unwrap().cell, before.cell);
        let direction = Direction::between(before.cell, next).unwrap();
        assert_eq!(
            simulation.tick(),
            vec![Event::StepFinished {
                creature: id,
                cell: next,
                direction
            },]
        );
        for _ in 0..20 {
            assert!(simulation.tick().is_empty());
        }
        assert!(simulation.world.debug_state(id).unwrap().route.is_empty());
        assert_eq!(simulation.group_states[1], GroupState::Calm);
    }
}

#[test]
fn debug_pursuit_snapshots_and_events_replay_identically() {
    let mut first = debug_encounter();
    let mut second = debug_encounter();
    for tick in 0..100 {
        for simulation in [&mut first, &mut second] {
            if tick == 30 {
                simulation.move_actor(Cell::new(32, 80)).unwrap();
            }
            if tick == 60 {
                simulation.remove_actor().unwrap();
            }
        }
        assert_eq!(first.tick(), second.tick());
        assert_eq!(first.debug_snapshot(), second.debug_snapshot());
    }
}

#[test]
fn a_static_debug_target_keeps_its_chase_after_the_group_becomes_calm() {
    // No retention-rectangle check for an unchanged destination: original 0x41213A.
    let mut simulation = debug_encounter();
    simulation.melee.leashes[1].left = 40;
    simulation.tick();
    simulation.roster[1].spawn.left = 40;
    simulation.tick();
    assert_eq!(simulation.group_states[1], GroupState::Calm);
    assert_eq!(
        simulation.debug_pursuit(1).unwrap().status,
        PursuitStatus::Approaching
    );
    advance_until(&mut simulation, |simulation| {
        simulation.debug_pursuit(1).unwrap().status == PursuitStatus::InRange
    });
    assert_eq!(simulation.debug_pursuit(1).unwrap().target_cell, (35, 80));
    assert_eq!(simulation.group_states[1], GroupState::MemberOutside);
}

#[test]
fn resetting_positions_clears_runtime_pursuit_state() {
    let mut simulation = debug_encounter();
    simulation.tick();
    assert!(simulation.debug_pursuit(1).is_some());
    let (fresh, _) = fixture(0x42, Cell::new(10, 80));
    simulation.adopt_positions_from(fresh);
    assert_eq!(simulation.debug_pursuit(1), None);
    assert_eq!(simulation.debug_player(), None);
}

#[test]
fn a_debug_target_leaving_range_is_reacquired_only_while_the_group_is_active() {
    for stays_inside in [false, true] {
        let (mut simulation, _) = fixture(0x42, Cell::new(10, 80));
        if stays_inside {
            simulation.roster[1].spawn.left = 40;
        }
        simulation.configure_melee_frames(vec![[0; 8]; 2]);
        place_debug(&mut simulation, (42, 80));
        simulation.tick();
        assert_eq!(
            simulation.debug_pursuit(1).unwrap().status,
            PursuitStatus::InRange
        );
        simulation.move_actor(Cell::new(41, 80)).unwrap();
        advance_until(&mut simulation, |simulation| {
            simulation.debug_pursuit(1).unwrap().status == PursuitStatus::Finished
        });
        assert_eq!(simulation.debug_pursuit(1).unwrap().target_cell, (42, 80));
        simulation.tick();
        if stays_inside {
            assert!(simulation.debug_pursuit(1).unwrap().status.active());
        } else {
            assert_eq!(simulation.group_states[1], GroupState::Calm);
            assert_eq!(
                simulation.debug_pursuit(1).unwrap().status,
                PursuitStatus::Finished
            );
        }
    }
}

#[test]
fn ambush_classes_never_chase_a_debug_target_beyond_their_neighbours() {
    for class_id in [0x44, 0x57] {
        let mut simulation = debug_encounter();
        simulation.roster[1].class_id = class_id;
        for _ in 0..5 {
            simulation.tick();
            assert_eq!(simulation.debug_pursuit(1), None);
        }
        simulation.remove_actor().unwrap();
        place_debug(&mut simulation, (42, 80));
        simulation.tick();
        assert_eq!(
            simulation.debug_pursuit(1).unwrap().status,
            PursuitStatus::InRange
        );
        simulation.move_actor(Cell::new(41, 80)).unwrap();
        advance_until(&mut simulation, |simulation| {
            simulation.debug_pursuit(1).unwrap().status == PursuitStatus::Finished
        });
        for _ in 0..10 {
            simulation.tick();
            assert_eq!(
                simulation.debug_pursuit(1).unwrap().status,
                PursuitStatus::Finished
            );
            assert_eq!(
                simulation
                    .world
                    .state(simulation.ids[1].unwrap())
                    .unwrap()
                    .cell,
                Cell::new(43, 80)
            );
        }
    }
}
