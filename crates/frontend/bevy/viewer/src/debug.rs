//! Runtime actor commands, journal export and development-only world geometry.
use std::path::Path;

use crate::{
    action::{DebugScope, ViewerWorkspace},
    *,
};
use bevy::prelude::*;
use kn_game::{
    debug::{DebugCommand, DebugScript, DebugSnapshot, GridRectangle, MovementDebugSnapshot},
    movement::grid::{Cell, Grid, ODD_ROW_SHIFT, STEP_X, STEP_Y},
    simulation::{PursuitError, PursuitStatus},
};
use kn_render::{CreatureSet, CreatureSim, DebugReplay, GameResource, MapCamera, ortho_scale};
use serde_json::{Value, json};

pub(crate) fn install(app: &mut App) {
    app.init_resource::<DebugReplay>()
        .init_resource::<Drawing>()
        .add_systems(
            Update,
            draw.after(CreatureSet::Nodes).after(kn_render::CameraApply),
        );
}

pub(crate) fn command(world: &mut World, command: DebugCommand) -> Result<(), String> {
    if !world.contains_resource::<DebugReplay>() || !world.contains_resource::<GameResource>() {
        let error = "Debug session is unavailable".to_owned();
        world
            .resource_mut::<ViewerWorkspace>()
            .set_feedback(format!("Error: {error}"));
        return Err(error);
    }

    let success = match &command {
        DebugCommand::Place { .. } => "Player placed",
        DebugCommand::Move { .. } => "Move requested",
        DebugCommand::Remove => "Player removed",
        DebugCommand::ResetPositions => "Positions reset; player removed; clock preserved",
        DebugCommand::ResetAnimation => "Special animations restarted",
    };
    let previous = world
        .get_resource::<DebugReplay>()
        .and_then(|replay| replay.last_command.as_ref())
        .map(|outcome| outcome.sequence);
    let result = world.resource_scope(|world, mut replay: Mut<DebugReplay>| {
        world.resource_scope(|world, mut sim: Mut<CreatureSim>| {
            replay.dispatch(&mut sim, &world.resource::<GameResource>().0, command)
        })
    });
    let feedback = world
        .resource::<DebugReplay>()
        .last_command
        .as_ref()
        .filter(|outcome| Some(outcome.sequence) != previous)
        .map(|outcome| {
            format!(
                "{} at logical tick {} (command {}){}",
                outcome
                    .result
                    .as_ref()
                    .err()
                    .map_or_else(|| success.into(), |error| format!("Error: {error}")),
                outcome.tick,
                outcome.sequence,
                if outcome.branched {
                    "; future history discarded"
                } else {
                    ""
                }
            )
        })
        .unwrap_or_else(|| {
            result
                .as_ref()
                .err()
                .map_or_else(|| success.into(), |error| format!("Error: {error}"))
        });
    world
        .resource_mut::<ViewerWorkspace>()
        .set_feedback(feedback);
    result
}

pub(crate) fn export(world: &mut World, path: &Path) -> Result<(), String> {
    let replay = world.resource::<DebugReplay>();
    let script = DebugScript {
        version: 1,
        map: replay.map.ok_or("No map session is available")?,
        hero: replay.hero,
        seed: replay.seed,
        profile: replay.profile.clone(),
        entries: replay.journal.entries().to_vec(),
        through_tick: world
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .map_or(0, Simulation::tick_number),
    };
    script.validate()?;
    let bytes = serde_json::to_vec_pretty(&script).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub(crate) fn projection(world: &World) -> (Value, Value) {
    let snapshot = world
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .map(Simulation::debug_snapshot);
    let player = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.player.clone());
    let replay = world.get_resource::<DebugReplay>();
    let workspace = world.resource::<ViewerWorkspace>();
    let overlays = workspace.debug_overlays;
    let debug = json!({"snapshot":snapshot.map(|snapshot|scoped_snapshot(snapshot,workspace.selected_creature,workspace.debug_scope)),"profile_error":replay.and_then(|replay|replay.profile_error.as_ref()),"overlays":{"routes":overlays[0],"targets":overlays[1],"group_states":overlays[2],"regions":overlays[3]},"scope":workspace.debug_scope,"feedback":workspace.feedback,"branch_revision":replay.map_or(0,|replay|replay.branch_revision),"command_count":replay.map_or(0,|replay|replay.journal.entries().len()),"applied_count":replay.map_or(0,|replay|replay.journal.applied_count()),"playhead":world.resource::<CreatureSim>().sim.as_ref().map_or(0,Simulation::tick_number),"furthest_tick":replay.map_or(0,|replay|replay.journal.furthest_tick()),"caught_up":replay.is_none_or(|replay|replay.caught_up),"replay_error":replay.and_then(|replay|replay.failed.as_ref()),"last_command":replay.and_then(|replay|replay.last_command.as_ref()).map(|outcome|json!({"tick":outcome.tick,"sequence":outcome.sequence,"branched":outcome.branched,"success":outcome.result.is_ok(),"error":outcome.result.as_ref().err()}))});
    (serde_json::to_value(player).unwrap_or(Value::Null), debug)
}

pub(crate) fn world_cell(point: Vec2) -> Option<Cell> {
    if !point.is_finite() {
        return None;
    }
    Grid::fine().cell_at(
        point.x.round_ties_even() as i32,
        (-point.y).round_ties_even() as i32,
    )
}

#[derive(Clone, PartialEq)]
struct Line {
    start: Vec2,
    end: Vec2,
    color: Color,
    layer: usize,
    width: f32,
}
#[derive(Clone, PartialEq)]
struct Label {
    position: Vec2,
    text: String,
    layer: usize,
    color: Color,
}
#[derive(Default)]
struct Geometry {
    lines: Vec<Line>,
    labels: Vec<Label>,
}
#[derive(Resource, Default)]
struct Drawing {
    lines: Vec<Entity>,
    labels: Vec<Entity>,
    last: Option<(DebugSnapshot, Option<usize>, DebugScope, f32)>,
    geometry: Geometry,
    last_flags: [bool; 4],
    inactive_hidden: bool,
}
#[derive(Component)]
struct DebugDrawing;

fn pixel(value: (i32, i32)) -> Vec2 {
    Vec2::new(value.0 as f32, -(value.1 as f32))
}
fn cell(value: (u32, u32)) -> Vec2 {
    pixel(Cell::new(value.0, value.1).centre())
}
fn signed_cell(column: i16, row: i16) -> Vec2 {
    let row = i32::from(row);
    Vec2::new(
        (i32::from(column) * STEP_X + (2 - (row & 1)) * ODD_ROW_SHIFT) as f32,
        -(row * STEP_Y + STEP_Y) as f32,
    )
}
fn state_name(value: u8) -> &'static str {
    match value {
        0 => "calm",
        1 => "player inside",
        32 => "member outside",
        _ => "unknown",
    }
}
impl Geometry {
    fn line(&mut self, a: Vec2, b: Vec2, color: Color, layer: usize, width: f32) {
        if a != b {
            self.lines.push(Line {
                start: a,
                end: b,
                color,
                layer,
                width,
            });
        }
    }
    fn cross(&mut self, center: Vec2, color: Color, layer: usize, scale: f32) {
        let size = 4.0 * scale;
        self.line(
            center - Vec2::X * size,
            center + Vec2::X * size,
            color,
            layer,
            scale,
        );
        self.line(
            center - Vec2::Y * size,
            center + Vec2::Y * size,
            color,
            layer,
            scale,
        );
    }
    fn movement(&mut self, movement: &MovementDebugSnapshot, scale: f32) {
        let mut previous = pixel(movement.pixel);
        for &target in &movement.route {
            let next = cell(target);
            self.line(previous, next, kn_ui::palette::ACCENT, 1, scale);
            previous = next;
        }
        if let Some(target) = movement.requested_target {
            self.cross(cell(target), kn_ui::palette::DANGER, 2, scale);
        }
        if let Some(target) = movement.accepted_target {
            self.cross(cell(target), kn_ui::palette::ACCENT, 2, scale * 0.75);
        }
    }
    fn rectangle(
        &mut self,
        rect: GridRectangle,
        group: usize,
        name: &str,
        color: Color,
        scale: f32,
    ) {
        let points = [
            signed_cell(rect.left, rect.top),
            signed_cell(rect.right, rect.top),
            signed_cell(rect.right, rect.bottom),
            signed_cell(rect.left, rect.bottom),
        ];
        for i in 0..4 {
            self.line(points[i], points[(i + 1) % 4], color, 4, scale);
        }
        self.labels.push(Label {
            position: points[0],
            text: format!(
                "G{group} {name} [{},{},{},{}]",
                rect.left, rect.top, rect.right, rect.bottom
            ),
            layer: 4,
            color,
        });
    }
}

fn scoped_snapshot(
    mut snapshot: DebugSnapshot,
    selected: Option<usize>,
    scope: DebugScope,
) -> DebugSnapshot {
    if scope == DebugScope::Selected {
        snapshot
            .creatures
            .retain(|creature| Some(creature.record_id) == selected);
        let groups = snapshot
            .creatures
            .iter()
            .map(|creature| creature.group)
            .collect::<std::collections::HashSet<_>>();
        snapshot
            .groups
            .retain(|group| groups.contains(&group.group));
    }
    snapshot
}

fn geometry(
    snapshot: &DebugSnapshot,
    selected: Option<usize>,
    scope: DebugScope,
    scale: f32,
) -> Geometry {
    let snapshot = scoped_snapshot(snapshot.clone(), selected, scope);
    let mut geometry = Geometry::default();
    let creatures = snapshot.creatures.iter().collect::<Vec<_>>();
    for creature in &creatures {
        if let Some(movement) = creature.movement.as_ref() {
            geometry.movement(movement, scale);
            if let Some(pursuit) = creature.pursuit {
                let status = match pursuit.status {
                    PursuitStatus::Pending => "pending",
                    PursuitStatus::Approaching => "approaching",
                    PursuitStatus::InRange => "in range",
                    PursuitStatus::Finished => "finished",
                    PursuitStatus::Refused(PursuitError::NoRoute) => "no route",
                    PursuitStatus::Refused(PursuitError::TargetUnavailable) => "target unavailable",
                };
                let active = matches!(
                    pursuit.status,
                    PursuitStatus::Pending | PursuitStatus::Approaching | PursuitStatus::InRange
                );
                if active {
                    geometry.line(
                        pixel(movement.pixel),
                        cell(pursuit.target_cell),
                        kn_ui::palette::DANGER,
                        2,
                        scale,
                    );
                }
                geometry.labels.push(Label {
                    position: pixel(movement.pixel) - Vec2::Y * 12.0 * scale,
                    text: format!(
                        "#{} -> Debug player #{}: {status}",
                        creature.record_id, pursuit.target_id
                    ),
                    layer: 2,
                    color: kn_ui::palette::TEXT,
                });
            }
            geometry.labels.push(Label {
                position: pixel(movement.pixel) + Vec2::Y * 12.0 * scale,
                text: format!(
                    "#{} G{} used {} ({}) @ {}; preview {} ({})",
                    creature.record_id,
                    creature.group,
                    creature.used_group_state,
                    state_name(creature.used_group_state),
                    creature
                        .used_group_tick
                        .map_or_else(|| "unevaluated".into(), |tick| format!("tick {tick}")),
                    creature.preview_group_state,
                    state_name(creature.preview_group_state)
                ),
                layer: 3,
                color: kn_ui::palette::TEXT,
            });
        }
    }
    for group in &snapshot.groups {
        {
            geometry.rectangle(
                group.spawn,
                group.group,
                "Spawn area",
                kn_ui::palette::ACCENT,
                scale,
            );
            geometry.rectangle(
                group.second_rectangle,
                group.group,
                "Pursuit retention area",
                kn_ui::palette::TEXT_MUTED,
                scale,
            );
        }
    }
    if let Some(player) = snapshot.player.as_ref() {
        geometry.movement(&player.movement, scale);
        let from = pixel(player.movement.pixel);
        let position = player.movement.step_to_pixel.map_or(from, |to| {
            from.lerp(
                pixel(to),
                (player.movement.step_elapsed as f32 / player.movement.step_ticks.max(1) as f32)
                    .clamp(0.0, 1.0),
            )
        });
        let radius = (6.0 * scale).max(12.0);
        let points = [
            position + Vec2::Y * radius,
            position + Vec2::X * radius,
            position - Vec2::Y * radius,
            position - Vec2::X * radius,
        ];
        for i in 0..4 {
            geometry.line(
                points[i],
                points[(i + 1) % 4],
                kn_ui::palette::ACCENT,
                0,
                2.0 * scale,
            );
        }
        geometry.cross(position, kn_ui::palette::TEXT, 0, scale);
        geometry.labels.push(Label {
            position: position + Vec2::Y * (radius + 10.0 * scale),
            text: "Debug player".into(),
            layer: 0,
            color: kn_ui::palette::TEXT,
        });
    }
    geometry
}

fn set_visible(entity: &mut EntityWorldMut, shown: bool) {
    let visibility = if shown {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if entity.get::<Visibility>() != Some(&visibility) {
        entity.insert((
            visibility,
            if shown {
                InheritedVisibility::VISIBLE
            } else {
                InheritedVisibility::HIDDEN
            },
        ));
    }
}

fn draw(world: &mut World) {
    let workspace = world.resource::<ViewerWorkspace>();
    let selected = workspace.selected_creature;
    let scope = workspace.debug_scope;
    let flags = workspace.debug_overlays;
    let present = world
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .and_then(Simulation::debug_player)
        .is_some();
    if !present && !flags.iter().any(|flag| *flag) {
        world.resource_scope(|world, mut drawing: Mut<Drawing>| {
            if !drawing.inactive_hidden {
                for &entity in drawing.lines.iter().chain(drawing.labels.iter()) {
                    set_visible(&mut world.entity_mut(entity), false);
                }
                drawing.inactive_hidden = true;
            }
        });
        return;
    }
    let snapshot = world
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .map(Simulation::debug_snapshot);
    let scale = world
        .query_filtered::<&Projection, With<MapCamera>>()
        .iter(world)
        .next()
        .map_or(1.0, ortho_scale);
    let fonts = world.resource::<KonungFonts>().regular.clone();
    world.resource_scope(|world, mut drawing: Mut<Drawing>| {
        let key = snapshot.map(|snapshot| (snapshot, selected, scope, scale));
        let changed = drawing.last != key;
        if !changed && drawing.last_flags == flags && !drawing.inactive_hidden {
            return;
        }
        if changed {
            drawing.geometry = key.as_ref().map_or_else(Geometry::default, |key| {
                geometry(&key.0, selected, scope, scale)
            });
            drawing.last = key;
        }
        drawing.last_flags = flags;
        drawing.inactive_hidden = false;
        while drawing.lines.len() < drawing.geometry.lines.len() {
            let entity = world
                .spawn((
                    DebugDrawing,
                    Sprite::from_color(Color::NONE, Vec2::ONE),
                    Transform::default(),
                    GlobalTransform::default(),
                    Visibility::Hidden,
                    InheritedVisibility::HIDDEN,
                    bevy::camera::visibility::NoFrustumCulling,
                ))
                .id();
            drawing.lines.push(entity);
        }
        while drawing.labels.len() < drawing.geometry.labels.len() {
            let entity = world
                .spawn((
                    DebugDrawing,
                    Text2d::new(""),
                    TextFont {
                        font: fonts.clone().into(),
                        font_size: FontSize::Px(12.0 * scale),
                        ..default()
                    },
                    TextColor(kn_ui::palette::TEXT),
                    Transform::default(),
                    GlobalTransform::default(),
                    Visibility::Hidden,
                    InheritedVisibility::HIDDEN,
                    bevy::camera::visibility::NoFrustumCulling,
                ))
                .id();
            drawing.labels.push(entity);
        }
        for (index, &entity) in drawing.lines.iter().enumerate() {
            let line = drawing.geometry.lines.get(index);
            let shown = line.is_some_and(|line| line.layer == 0 || flags[line.layer - 1]);
            let mut entity = world.entity_mut(entity);
            set_visible(&mut entity, shown);
            if changed && let Some(line) = line {
                let delta = line.end - line.start;
                let transform =
                    Transform::from_translation(((line.start + line.end) * 0.5).extend(910.0))
                        .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x)));
                entity.insert((
                    Sprite::from_color(line.color, Vec2::new(delta.length(), line.width)),
                    transform,
                    GlobalTransform::from(transform),
                ));
            }
        }
        for (index, &entity) in drawing.labels.iter().enumerate() {
            let label = drawing.geometry.labels.get(index);
            let shown = label.is_some_and(|label| label.layer == 0 || flags[label.layer - 1]);
            let mut entity = world.entity_mut(entity);
            set_visible(&mut entity, shown);
            if changed && let Some(label) = label {
                let transform = Transform::from_translation(label.position.extend(920.0));
                entity.insert((
                    Text2d::new(label.text.clone()),
                    TextColor(label.color),
                    TextFont {
                        font: fonts.clone().into(),
                        font_size: FontSize::Px(12.0 * scale),
                        ..default()
                    },
                    transform,
                    GlobalTransform::from(transform),
                ));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1_data::{
        game_state::{Character, GameState, Group, Rect},
        map::KonungMap,
    };
    use kn_game::debug::DebugPlayerProfile;

    fn simulation() -> Simulation {
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
                        kind: 0x41,
                        spawn: Rect {
                            left: -2,
                            top: -1,
                            right: 12,
                            bottom: 15,
                        },
                        leash: Rect {
                            left: -5,
                            top: -4,
                            right: 18,
                            bottom: 20,
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
                        class_id: 0x50,
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
            vec![0],
            vec![],
            42,
            None,
        )
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
    #[derive(Resource, Default)]
    struct Changes {
        sprites: usize,
        text: usize,
    }
    fn count_changes(
        sprites: Query<Entity, Changed<Sprite>>,
        text: Query<Entity, Changed<Text2d>>,
        mut changes: ResMut<Changes>,
    ) {
        changes.sprites = sprites.iter().count();
        changes.text = text.iter().count();
    }
    fn app() -> App {
        let mut app = App::new();
        let mut fonts = Assets::<Font>::default();
        let handles = kn_ui::konung_fonts(&mut fonts);
        app.add_plugins(MinimalPlugins)
            .insert_resource(fonts)
            .insert_resource(handles)
            .insert_resource(CreatureSim {
                sim: Some(simulation()),
                ..default()
            })
            .insert_resource(ViewerWorkspace {
                selected_creature: Some(2),
                debug_overlays: [true; 4],
                ..default()
            })
            .insert_resource(Viewer {
                maps: vec![],
                selected: 0,
                overlays: [false; 4],
                playback: PlayState::Paused,
                constant_day: false,
            })
            .insert_resource(Stats {
                built_ms: 123,
                ..default()
            })
            .init_resource::<Rebuild>()
            .init_resource::<TextureCache>()
            .init_resource::<Drawing>()
            .init_resource::<Changes>()
            .add_systems(Update, (draw, count_changes).chain());
        app.world_mut().spawn((
            MapCamera,
            Projection::Orthographic(OrthographicProjection::default_2d()),
        ));
        app
    }

    #[test]
    fn clicks_use_existing_fine_grid_inverse_without_clamping_off_grid_points() {
        for row in 0..kn_game::movement::grid::FINE_HEIGHT {
            for column in 0..kn_game::movement::grid::FINE_WIDTH {
                let expected = Cell::new(column, row);
                assert_eq!(world_cell(pixel(expected.centre())), Some(expected));
            }
        }
        assert_eq!(world_cell(Vec2::new(-500.0, 0.0)), None);
        assert_eq!(world_cell(Vec2::new(0.0, 500.0)), None);
        assert_eq!(world_cell(Vec2::splat(f32::NAN)), None);
    }

    #[test]
    fn raw_regions_keep_signed_coordinates_and_shared_scope_facts() {
        assert_eq!(signed_cell(-1, -1), Vec2::new(-29.0, 0.0));
        let snapshot = simulation().debug_snapshot();
        let selected = scoped_snapshot(snapshot.clone(), Some(2), DebugScope::Selected);
        assert_eq!(selected.creatures.len(), 1);
        assert_eq!(selected.groups[0].second_rectangle.left, -5);
        let geometry = geometry(&snapshot, Some(2), DebugScope::Selected, 1.0);
        assert!(
            geometry
                .lines
                .iter()
                .any(|line| line.layer == 4 && line.start.x < 0.0)
        );
        assert!(
            geometry
                .labels
                .iter()
                .any(|label| label.text.contains("Pursuit retention area"))
        );
        assert!(
            scoped_snapshot(snapshot, None, DebugScope::Selected)
                .groups
                .is_empty()
        );
    }

    #[test]
    fn pursuit_targets_name_the_runtime_actor_in_both_scopes() {
        let mut simulation = simulation();
        simulation
            .apply_debug_command(
                &DebugCommand::Place { cell: (10, 10) },
                Some(&profile()),
                || unreachable!(),
            )
            .unwrap();
        simulation.tick();
        let snapshot = simulation.debug_snapshot();
        let creature = &snapshot.creatures[0];
        let pursuit = creature
            .pursuit
            .expect("hostile group acquires the debug player");
        assert_eq!(pursuit.target_id, snapshot.player.as_ref().unwrap().id);
        for scope in [DebugScope::Selected, DebugScope::All] {
            let geometry = geometry(&snapshot, Some(creature.record_id), scope, 1.0);
            assert!(geometry.labels.iter().any(|label| {
                label.layer == 2
                    && label
                        .text
                        .contains(&format!("Debug player #{}", pursuit.target_id))
            }));
            assert!(geometry.lines.iter().any(|line| line.layer == 2
                && line.start == pixel(creature.movement.as_ref().unwrap().pixel)
                && line.end == cell(pursuit.target_cell)));
        }
        assert!(
            geometry(&snapshot, None, DebugScope::Selected, 1.0)
                .labels
                .iter()
                .all(|label| !label.text.contains("-> Debug player"))
        );
        let record_id = creature.record_id;
        let json = serde_json::to_value(scoped_snapshot(
            snapshot,
            Some(record_id),
            DebugScope::Selected,
        ))
        .unwrap();
        assert_eq!(
            json["creatures"][0]["pursuit"]["target_id"],
            pursuit.target_id
        );
    }

    #[test]
    fn overlay_toggle_is_visibility_only_and_unavailable_simulation_hides_the_pool() {
        let mut app = app();
        app.update();
        let mut entities = app
            .world_mut()
            .query_filtered::<Entity, With<DebugDrawing>>();
        let initial = entities
            .iter(app.world())
            .collect::<std::collections::HashSet<_>>();
        assert!(!initial.is_empty());
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .debug_overlays = [false; 4];
        app.update();
        assert_eq!(app.world().resource::<Changes>().sprites, 0);
        assert_eq!(app.world().resource::<Changes>().text, 0);
        assert_eq!(
            entities
                .iter(app.world())
                .collect::<std::collections::HashSet<_>>(),
            initial
        );
        assert!(!app.world().resource::<Rebuild>().map);
        assert_eq!(app.world().resource::<TextureCache>().len(), 0);
        assert_eq!(app.world().resource::<Stats>().built_ms, 123);
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .debug_overlays = [true; 4];
        app.update();
        app.world_mut().resource_mut::<CreatureSim>().sim = None;
        app.update();
        for entity in entities.iter(app.world()) {
            assert!(
                !app.world()
                    .entity(entity)
                    .get::<InheritedVisibility>()
                    .unwrap()
                    .get()
            );
        }
    }

    #[test]
    fn marker_remains_visible_with_characters_hidden_and_removal_hides_it() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = None;
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .debug_overlays = [false; 4];
        app.world_mut()
            .resource_mut::<CreatureSim>()
            .sim
            .as_mut()
            .unwrap()
            .apply_debug_command(
                &DebugCommand::Place { cell: (10, 10) },
                Some(&profile()),
                || Ok(simulation()),
            )
            .unwrap();
        app.update();
        let mut visuals = app
            .world_mut()
            .query_filtered::<&InheritedVisibility, With<DebugDrawing>>();
        assert!(visuals.iter(app.world()).any(|visible| visible.get()));
        app.world_mut()
            .resource_mut::<CreatureSim>()
            .sim
            .as_mut()
            .unwrap()
            .apply_debug_command(&DebugCommand::Remove, None, || Ok(simulation()))
            .unwrap();
        app.update();
        assert!(visuals.iter(app.world()).all(|visible| !visible.get()));
    }

    #[test]
    fn current_dispatch_error_cannot_reuse_old_success_feedback() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../original/k1/eng/game");
        if !path.is_dir() {
            return;
        }
        let game = Game::open(path).unwrap();
        let map = MapRef {
            source: MapSource::Game,
            number: 17,
        };
        let simulation = game.simulation(map, SIM_SEED).unwrap();
        let mut replay = DebugReplay::default();
        replay.start(&game, map, SIM_SEED, None);
        let mut world = World::new();
        world.insert_resource(GameResource(game));
        world.insert_resource(replay);
        world.insert_resource(CreatureSim {
            sim: Some(simulation),
            ..default()
        });
        world.init_resource::<ViewerWorkspace>();
        command(&mut world, DebugCommand::Place { cell: (44, 48) }).unwrap();
        world.resource_mut::<DebugReplay>().failed = Some("synthetic fatal replay".into());
        assert!(command(&mut world, DebugCommand::Move { cell: (42, 46) }).is_err());
        assert!(
            world
                .resource::<ViewerWorkspace>()
                .feedback
                .contains("Replay is halted")
        );
        assert!(
            !world
                .resource::<ViewerWorkspace>()
                .feedback
                .starts_with("Command applied")
        );
    }

    #[test]
    #[ignore = "Opt-in actual-map overlay and debug projection CPU measurement"]
    fn measure_actual_map_overlay_cost() {
        let path = std::env::var("GAME_DIR").expect("GAME_DIR must name local game files");
        let number = std::env::var("MAP")
            .unwrap_or_else(|_| "17".into())
            .parse()
            .unwrap();
        let game = Game::open(path).unwrap();
        let map = MapRef {
            source: MapSource::Game,
            number,
        };
        let profile = game.debug_player_profile(map, None).unwrap();
        let mut simulation = game.simulation(map, SIM_SEED).unwrap();
        simulation
            .apply_debug_command(
                &DebugCommand::Place { cell: (44, 48) },
                Some(&profile),
                || {
                    game.simulation(map, SIM_SEED)
                        .map_err(|error| error.to_string())
                },
            )
            .unwrap();
        simulation
            .apply_debug_command(
                &DebugCommand::Move { cell: (42, 46) },
                Some(&profile),
                || {
                    game.simulation(map, SIM_SEED)
                        .map_err(|error| error.to_string())
                },
            )
            .unwrap();
        for _ in 0..600 {
            simulation.tick();
        }
        let snapshot = simulation.debug_snapshot();
        let mut app = app();
        app.world_mut().resource_mut::<CreatureSim>().sim = Some(simulation);
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .debug_scope = DebugScope::All;
        app.world_mut().init_resource::<DebugReplay>();
        draw(app.world_mut());
        let iterations = 2000;
        for operation in [
            "snapshot",
            "geometry",
            "cached_draw",
            "toggle_draw",
            "projection",
        ] {
            let started = std::time::Instant::now();
            for index in 0..iterations {
                match operation {
                    "snapshot" => {
                        std::hint::black_box(
                            app.world()
                                .resource::<CreatureSim>()
                                .sim
                                .as_ref()
                                .unwrap()
                                .debug_snapshot(),
                        );
                    }
                    "geometry" => {
                        std::hint::black_box(geometry(&snapshot, None, DebugScope::All, 1.0));
                    }
                    "cached_draw" => draw(app.world_mut()),
                    "toggle_draw" => {
                        app.world_mut()
                            .resource_mut::<ViewerWorkspace>()
                            .debug_overlays = [index % 2 == 0; 4];
                        draw(app.world_mut());
                    }
                    "projection" => {
                        std::hint::black_box(projection(app.world()));
                    }
                    _ => unreachable!(),
                }
            }
            println!(
                "ACTUAL_OVERLAY map={number} creatures={} groups={} player=true operation={operation} mean_us={:.3}",
                snapshot.creatures.len(),
                snapshot.groups.len(),
                started.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
            );
        }
    }

    #[test]
    #[ignore = "Opt-in overlay geometry CPU measurement"]
    fn measure_overlay_geometry_cost() {
        let snapshot = simulation().debug_snapshot();
        let mut large = snapshot.clone();
        large.creatures = (0..200)
            .map(|index| {
                let mut creature = snapshot.creatures[0].clone();
                creature.record_id = index;
                let movement = creature.movement.as_mut().unwrap();
                movement.route = (0..20)
                    .map(|step| (step + 1, index as u32 % 100 + 1))
                    .collect();
                creature
            })
            .collect();
        for (name, scope, selected) in [
            ("selected", DebugScope::Selected, Some(2)),
            ("all", DebugScope::All, None),
        ] {
            let started = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(geometry(&large, selected, scope, 1.0));
            }
            let geometry_us = started.elapsed().as_secs_f64() * 1e6 / 1000.0;
            let started = std::time::Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(large.clone());
            }
            let clone_us = started.elapsed().as_secs_f64() * 1e6 / 1000.0;
            println!(
                "OVERLAY scope={name} creatures=200 route_cells=20 geometry_us={geometry_us:.3} snapshot_clone_us={clone_us:.3}"
            );
        }
    }
}
