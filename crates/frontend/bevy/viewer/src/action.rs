//! Shared actions and read projection for the workspace and session control.
use std::collections::VecDeque;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    #[default]
    Select,
    Pan,
    DebugPlayer,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugScope {
    #[default]
    Selected,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewerAction {
    State {},
    Creatures {
        #[serde(default)]
        search: String,
    },
    Select {
        #[serde(deserialize_with = "required_selection")]
        id: Option<usize>,
    },
    Pick {
        x: f32,
        y: f32,
    },
    Mode {
        mode: InteractionMode,
    },
    Map {
        source: String,
        number: u32,
    },
    RetryMap {},
    Display {
        layer: String,
        visible: bool,
    },
    Playback {
        playing: bool,
    },
    TogglePlayback {},
    Speed {
        rate: f32,
    },
    Step {
        ticks: i64,
    },
    Seek {
        tick: u64,
    },
    ConstantDay {
        enabled: bool,
    },
    ResetPositions {},
    ResetAnimation {},
    FocusSelection {},
    FitMap {},
    Window {
        width: u32,
        height: u32,
    },
    Camera {
        x: Option<f32>,
        y: Option<f32>,
        zoom: Option<usize>,
    },
    Capture {
        path: PathBuf,
    },
    DebugClick {
        column: u32,
        row: u32,
    },
    DebugPlace {
        column: u32,
        row: u32,
    },
    DebugMove {
        column: u32,
        row: u32,
    },
    DebugRemove {},
    DebugDisplay {
        layer: String,
        visible: bool,
    },
    DebugScope {
        scope: DebugScope,
    },
    ExportCommands {
        path: PathBuf,
    },
    Escape {},
}

fn required_selection<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<usize>, D::Error> {
    Option::<usize>::deserialize(deserializer)
}

#[derive(Resource, Default)]
pub struct ActionQueue(pub VecDeque<ViewerAction>);

#[derive(Resource, Default)]
pub struct ViewerWorkspace {
    pub selected_creature: Option<usize>,
    pub hovered_creature: Option<usize>,
    pub mode: InteractionMode,
    pub display_open: bool,
    pub time_open: bool,
    pub diagnostics_open: bool,
    pub feedback: String,
    pub feedback_revision: u64,
    pub map_search: String,
    pub creature_search: String,
    pub keyboard_captured: bool,
    pub debug_overlays: [bool; 4],
    pub debug_scope: DebugScope,
}

impl ViewerWorkspace {
    pub fn set_feedback(&mut self, feedback: impl Into<String>) {
        self.feedback = feedback.into();
        self.feedback_revision = self.feedback_revision.wrapping_add(1);
    }
}

#[derive(Resource, Default)]
pub struct ReadProjection(pub Value);

pub(crate) fn install(app: &mut App) {
    app.init_resource::<ActionQueue>()
        .init_resource::<ViewerWorkspace>()
        .init_resource::<ReadProjection>()
        .add_systems(
            Update,
            drain_actions
                .before(apply_playback)
                .after(CameraInput)
                .after(AnimSet::Clock),
        )
        .add_systems(
            PostUpdate,
            project_state
                .after(bevy::transform::TransformSystems::Propagate)
                .after(crate::input::pick)
                .after(crate::control::finish),
        );
}

pub(crate) fn drain_actions(world: &mut World) {
    if world
        .get_resource::<crate::control::ControlServer>()
        .is_some_and(|server| server.busy())
    {
        let escape = world
            .resource::<ActionQueue>()
            .0
            .iter()
            .position(|action| matches!(action, ViewerAction::Escape {}));
        if let Some(index) = escape {
            let action = world
                .resource_mut::<ActionQueue>()
                .0
                .remove(index)
                .expect("the queued Escape is present");
            let _ = apply_action(world, action);
        }
        return;
    }
    let actions = world
        .resource_mut::<ActionQueue>()
        .0
        .drain(..)
        .collect::<Vec<_>>();
    for action in actions {
        let _ = apply_action(world, action);
    }
}

pub(crate) fn apply_action(world: &mut World, action: ViewerAction) -> Result<(), String> {
    let read_only = matches!(
        action,
        ViewerAction::State {} | ViewerAction::Creatures { .. }
    );
    let feedback = match &action {
        ViewerAction::ResetPositions {} => "Creature positions reset; clock preserved",
        ViewerAction::ResetAnimation {} => {
            "Creature animation restarted; positions and clock preserved"
        }
        ViewerAction::Select { id: Some(_) } => "Creature selected",
        ViewerAction::Select { id: None } => "Selection cleared",
        ViewerAction::Mode {
            mode: InteractionMode::Select,
        } => "Select tool active",
        ViewerAction::Mode {
            mode: InteractionMode::Pan,
        } => "Pan tool active",
        ViewerAction::Speed { .. } => "Playback speed updated",
        ViewerAction::Step { .. } => "Stepped one tick; playback paused",
        ViewerAction::Seek { .. } => "Seeking simulation; playback paused",
        ViewerAction::ConstantDay { enabled: true } => "Constant day enabled; clock continues",
        ViewerAction::ConstantDay { enabled: false } => "Day/night cycle enabled",
        ViewerAction::Map { .. } | ViewerAction::RetryMap {} => "Loading requested map",
        ViewerAction::FocusSelection {} => "Camera focused on selection",
        ViewerAction::FitMap {} => "Camera fitted to map",
        ViewerAction::Escape {} => "Select tool active; disclosures closed",
        _ => "Applied",
    };
    let debug_feedback = matches!(
        action,
        ViewerAction::DebugClick { .. }
            | ViewerAction::DebugPlace { .. }
            | ViewerAction::DebugMove { .. }
            | ViewerAction::DebugRemove {}
    ) || (world.contains_resource::<kn_render::DebugReplay>()
        && matches!(
            action,
            ViewerAction::ResetPositions {} | ViewerAction::ResetAnimation {}
        ));
    let result = reduce_action(world, action);
    if !read_only
        && !debug_feedback
        && let Some(mut workspace) = world.get_resource_mut::<ViewerWorkspace>()
    {
        workspace.set_feedback(
            result
                .as_ref()
                .err()
                .map(|error| format!("Error: {error}"))
                .unwrap_or_else(|| feedback.into()),
        );
    }
    result
}

fn reduce_action(world: &mut World, action: ViewerAction) -> Result<(), String> {
    match action {
        ViewerAction::State {} | ViewerAction::Creatures { .. } => {}
        ViewerAction::Select { id } => {
            if let Some(id) = id {
                let exists = world
                    .resource::<CreatureSim>()
                    .sim
                    .as_ref()
                    .and_then(|sim| sim.inspect_creature(id))
                    .is_some();
                if !exists {
                    return Err(format!("Creature {id} is absent from the displayed map"));
                }
            }
            world.resource_mut::<ViewerWorkspace>().selected_creature = id;
        }
        ViewerAction::Mode { mode } => world.resource_mut::<ViewerWorkspace>().mode = mode,
        ViewerAction::Map { source, number } => {
            let source = match source.as_str() {
                "game" => MapSource::Game,
                "tutorial" => MapSource::Tutorial,
                _ => return Err("Source must be game or tutorial".into()),
            };
            let index = world
                .resource::<Viewer>()
                .maps
                .iter()
                .position(|map| map.source == source && map.number == number)
                .ok_or("Map is unavailable")?;
            world.resource_mut::<Viewer>().selected = index;
            world.resource_mut::<Rebuild>().map = true;
        }
        ViewerAction::RetryMap {} => world.resource_mut::<Rebuild>().map = true,
        ViewerAction::Display { layer, visible } => {
            let overlay = match layer.as_str() {
                "foot" => Overlay::Foot,
                "lights" => Overlay::Lights,
                "markers" => Overlay::Markers,
                "characters" => Overlay::Characters,
                _ => return Err("Unknown display layer".into()),
            };
            world.resource_mut::<Viewer>().set(overlay, visible);
        }
        ViewerAction::Playback { playing } => {
            world.resource_mut::<Viewer>().playback = playback_for(playing)
        }
        ViewerAction::TogglePlayback {} => {
            let state = world.resource::<Viewer>().playback;
            world.resource_mut::<Viewer>().playback = toggle_playback(state);
        }
        ViewerAction::Speed { rate } => {
            if ![0.25, 0.5, 1.0, 2.0, 4.0, 8.0].contains(&rate) {
                return Err("Speed must be 0.25, 0.5, 1, 2, 4 or 8".into());
            }
            world
                .resource_mut::<Clock>()
                .set_rate(rate)
                .map_err(|error| error.to_string())?;
        }
        ViewerAction::Step { ticks } => {
            if world.resource::<CreatureSim>().sim.is_some() {
                world.resource_mut::<CreatureSim>().error = None;
            }
            world.resource_mut::<Viewer>().playback = PlayState::Paused;
            world.resource_mut::<Clock>().step(ticks);
        }
        ViewerAction::Seek { tick } => {
            align_absolute_seek(&mut world.resource_mut::<CreatureSim>());
            if world.resource::<CreatureSim>().sim.is_some() {
                world.resource_mut::<CreatureSim>().error = None;
            }
            world.resource_mut::<Viewer>().playback = PlayState::Paused;
            world.resource_mut::<Clock>().ticks = tick;
            world.resource_mut::<Clock>().sub_tick = 0.0;
        }
        ViewerAction::ConstantDay { enabled } => {
            world.resource_mut::<Viewer>().constant_day = enabled
        }
        ViewerAction::ResetAnimation {} => {
            if world.contains_resource::<kn_render::DebugReplay>() {
                crate::debug::command(world, kn_game::debug::DebugCommand::ResetAnimation)?;
            } else {
                reset_creature_animations(&mut world.resource_mut::<CreatureSim>());
            }
        }
        ViewerAction::ResetPositions {} => {
            if world.contains_resource::<kn_render::DebugReplay>() {
                crate::debug::command(world, kn_game::debug::DebugCommand::ResetPositions)?;
                return Ok(());
            }
            let map = world.resource::<Stats>().map.ok_or("No map displayed")?;
            let tick = world.resource::<Clock>().ticks;
            world.resource_scope(|world, mut sim: Mut<CreatureSim>| {
                reset_creature_positions(
                    &world.resource::<GameResource>().0,
                    map,
                    SIM_SEED,
                    tick,
                    &mut sim,
                )
            });
        }
        ViewerAction::FitMap {} => {
            let bounds = *world.resource::<CameraBounds>();
            let mut pending = world.resource_mut::<PendingFrame>();
            pending.center = Some((bounds.min + bounds.max) * 0.5);
            pending.bounds = Some((bounds.min, bounds.max));
        }
        ViewerAction::FocusSelection {} => {
            let id = world
                .resource::<ViewerWorkspace>()
                .selected_creature
                .ok_or("Select a creature first")?;
            let position = world
                .query::<(&CreatureNode, &Transform)>()
                .iter(world)
                .find(|(node, _)| node.id as usize == id)
                .map(|(_, transform)| transform.translation.truncate())
                .ok_or("Selected creature has no visible position")?;
            world.resource_mut::<PendingFrame>().center = Some(position);
        }
        ViewerAction::Window { width, height } => {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err("Window width and height must be in 1..=8192 logical pixels".into());
            }
            let mut windows = world.query::<&mut Window>();
            let mut window = windows
                .iter_mut(world)
                .next()
                .ok_or("No viewer window is available")?;
            window.resolution.set(width as f32, height as f32);
        }
        ViewerAction::Camera { x, y, zoom } => {
            if x.is_some_and(|v| !v.is_finite()) || y.is_some_and(|v| !v.is_finite()) {
                return Err("Camera coordinates must be finite".into());
            }
            if let Some(zoom) = zoom {
                if zoom >= kn_render::ZOOM_LEVELS.len() {
                    return Err("Zoom index is outside the ladder".into());
                }
                world.resource_mut::<CameraState>().zoom = zoom;
            }
            for mut transform in world
                .query_filtered::<&mut Transform, With<MapCamera>>()
                .iter_mut(world)
            {
                if let Some(x) = x {
                    transform.translation.x = x;
                }
                if let Some(y) = y {
                    transform.translation.y = y;
                }
            }
        }
        ViewerAction::DebugClick { column, row } => {
            let cell = (column, row);
            let present = world
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .and_then(Simulation::debug_player)
                .is_some();
            crate::debug::command(
                world,
                if present {
                    kn_game::debug::DebugCommand::Move { cell }
                } else {
                    kn_game::debug::DebugCommand::Place { cell }
                },
            )?;
        }
        ViewerAction::DebugPlace { column, row } => crate::debug::command(
            world,
            kn_game::debug::DebugCommand::Place {
                cell: (column, row),
            },
        )?,
        ViewerAction::DebugMove { column, row } => crate::debug::command(
            world,
            kn_game::debug::DebugCommand::Move {
                cell: (column, row),
            },
        )?,
        ViewerAction::DebugRemove {} => {
            crate::debug::command(world, kn_game::debug::DebugCommand::Remove)?
        }
        ViewerAction::DebugDisplay { layer, visible } => {
            let index = match layer.as_str() {
                "routes" => 0,
                "targets" => 1,
                "group_states" => 2,
                "regions" => 3,
                _ => return Err("Unknown debug display layer".into()),
            };
            world.resource_mut::<ViewerWorkspace>().debug_overlays[index] = visible;
        }
        ViewerAction::DebugScope { scope } => {
            world.resource_mut::<ViewerWorkspace>().debug_scope = scope
        }
        ViewerAction::ExportCommands { path } => crate::debug::export(world, &path)?,
        ViewerAction::Escape {} => {
            if let Some(mut scrub) = world.get_resource_mut::<ScrubState>()
                && scrub.active
            {
                scrub.release = true;
            }
            if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
                focus.clear();
            }
            let mut workspace = world.resource_mut::<ViewerWorkspace>();
            workspace.mode = InteractionMode::Select;
            workspace.display_open = false;
            workspace.time_open = false;
            workspace.diagnostics_open = false;
        }
        ViewerAction::Capture { .. } => {
            return Err("Capture is available through session control".into());
        }
        ViewerAction::Pick { x, y } => {
            if !x.is_finite() || !y.is_finite() {
                return Err("Pick coordinates must be finite".into());
            }
            let mut system = bevy::ecs::system::SystemState::<(
                kn_render::CreaturePicker,
                Query<(&Camera, &GlobalTransform), With<MapCamera>>,
            )>::new(world);
            let (picker, cameras) = system.get(world).map_err(|error| error.to_string())?;
            let (camera, transform) = cameras.single().map_err(|error| error.to_string())?;
            let id = picker.pick_screen(Vec2::new(x, y), camera, transform);
            world.resource_mut::<ViewerWorkspace>().selected_creature = id;
        }
    }
    Ok(())
}

pub(crate) fn project_state(world: &mut World) {
    let state = state_value(world);
    world.resource_mut::<ReadProjection>().0 = state;
}

fn attack_value(order: kn_game::simulation::AttackOrder) -> Value {
    use kn_game::simulation::{AttackError, AttackStatus};
    let (status, error) = match order.status {
        AttackStatus::Pending => ("pending", None),
        AttackStatus::Approaching => ("approaching", None),
        AttackStatus::Attacking => ("attacking", None),
        AttackStatus::Finished => ("finished", None),
        AttackStatus::Refused(error) => (
            "refused",
            Some(match error {
                AttackError::AttackerUnavailable => "attacker_unavailable",
                AttackError::TargetUnavailable => "target_unavailable",
                AttackError::InvalidTarget => "invalid_target",
                AttackError::MissingAnimation => "missing_animation",
                AttackError::NoRoute => "no_route",
            }),
        ),
    };
    json!({"target_id":order.target_id,"status":status,"error":error})
}

fn pursuit_value(order: kn_game::simulation::DebugPursuit) -> Value {
    use kn_game::simulation::{PursuitError, PursuitStatus};
    let (status, error) = match order.status {
        PursuitStatus::Pending => ("pending", None),
        PursuitStatus::Approaching => ("approaching", None),
        PursuitStatus::InRange => ("in_range", None),
        PursuitStatus::Finished => ("finished", None),
        PursuitStatus::Refused(error) => (
            "refused",
            Some(match error {
                PursuitError::TargetUnavailable => "target_unavailable",
                PursuitError::NoRoute => "no_route",
            }),
        ),
    };
    json!({"target_id":order.target_id,"target_cell":order.target_cell,"status":status,"error":error})
}

pub(crate) fn state_value(world: &mut World) -> Value {
    let viewer = world.resource::<Viewer>();
    let requested = viewer
        .maps
        .get(viewer.selected)
        .map(|map| json!({"source":map.source.to_string().to_lowercase(),"number":map.number}));
    let playback = playback_checked(viewer);
    let overlays = viewer.overlays;
    let constant_day = viewer.constant_day;
    let stats = world.resource::<Stats>();
    let displayed = stats
        .map
        .map(|map| json!({"source":map.source.to_string().to_lowercase(),"number":map.number}));
    let error = stats.error.clone();
    let map_facts = json!({"unique_textures":stats.unique_textures,"cached_textures":stats.cached_textures,"animated_pieces":stats.animated_pieces,"animation_frames":stats.animation_frames,"built_ms":stats.built_ms,"texture_problems":stats.problems.len()});
    let workspace = world.resource::<ViewerWorkspace>();
    let selected = workspace.selected_creature;
    let mode = workspace.mode;
    let hovered = workspace.hovered_creature;
    let workspace_state = json!({"display_open":workspace.display_open,"time_open":workspace.time_open,"diagnostics_open":workspace.diagnostics_open,"map_search":workspace.map_search,"creature_search":workspace.creature_search,"feedback":workspace.feedback});
    let creatures = world.resource::<CreatureSim>().sim.as_ref().map(|sim| {
        let states=sim.states();
        sim.inspect_creatures().into_iter().map(|creature| {
            let state=states.iter().find(|state|state.id==creature.id);
            let stage=creature.animation_stage.map(|stage|match stage {
                kn_game::simulation::CreatureStage::Special { .. } => "special",
                kn_game::simulation::CreatureStage::Melee { .. } => "melee",
                kn_game::simulation::CreatureStage::Idle => "idle",
            });
            let special_frame=creature.animation_stage.and_then(|stage|match stage {kn_game::simulation::CreatureStage::Special {frame}=>Some(frame),_=>None});
            let melee_frame=creature.animation_stage.and_then(|stage|match stage {kn_game::simulation::CreatureStage::Melee {frame}=>Some(frame),_=>None});
            let attack=creature.attack.map(attack_value);
            let pursuit=creature.pursuit.map(pursuit_value);
            let group_state=match creature.group_state {kn_game::simulation::GroupState::Calm=>"calm",kn_game::simulation::GroupState::PlayerInside=>"player_inside",kn_game::simulation::GroupState::MemberOutside=>"member_outside"};
            json!({"id":creature.id,"label":creature.label,"recorded_state":creature.recorded_state,"animation_state":creature.animation_state,"animation_stage":stage,"animation_special_frame":special_frame,"animation_melee_frame":melee_frame,"attack":attack,"pursuit":pursuit,"walk_state":creature.walk_state.map(|state|format!("{state:?}").to_lowercase()),"group":creature.group,"group_state":group_state,"group_state_value":creature.group_state.byte(),"group_state_tick":creature.group_state_tick,"cell":state.map(|state|state.cell),"pixel":state.map(|state|state.pixel),"direction":state.and_then(|state|state.direction)})
        }).collect::<Vec<_>>()
    }).unwrap_or_default();
    let mut layers_query =
        world.query::<(&CreatureSprite, Option<&Animator>, &InheritedVisibility)>();
    let animation = world.get_resource::<CreatureAnim>();
    let mut render_layers=layers_query.iter(world).filter(|(tag,_,_)|Some(tag.creature_id)==selected && !tag.is_shadow).map(|(tag,animator,visible)| {
        let clip=animator.and_then(|animator|animation.and_then(|animation|clip_identity(animation,tag,&animator.sequence))).map(|(kind,_,_)|kind).unwrap_or_else(||if animator.is_some() {"unidentified".into()} else {"static".into()});
        json!({"layer":tag.layer,"clip":clip,"frame":animator.and_then(Animator::shown),"visible":visible.get()})
    }).collect::<Vec<_>>();
    render_layers.sort_by_key(|layer| layer["layer"].as_u64().unwrap_or(0));
    let mut pieces_query = world.query_filtered::<&InheritedVisibility, With<ScenePiece>>();
    let built = pieces_query.iter(world).count();
    let visible = pieces_query
        .iter(world)
        .filter(|visibility| visibility.get())
        .count();
    let creature_summary = world
        .get_resource::<CreatureAnimReport>()
        .map(creature_report);
    let fps = world
        .get_resource::<DiagnosticsStore>()
        .and_then(|frames| frames.get(&FrameTimeDiagnosticsPlugin::FPS))
        .and_then(|fps| fps.smoothed());
    let camera = world
        .query_filtered::<&Transform, With<MapCamera>>()
        .iter(world)
        .next()
        .map(|transform| json!({"x":transform.translation.x,"y":transform.translation.y}));
    let viewport = world
        .get_resource::<kn_render::MapViewport>()
        .and_then(|viewport| viewport.logical_rect)
        .map(|rect| json!({"min":[rect.min.x,rect.min.y],"max":[rect.max.x,rect.max.y]}));
    let window=world.query::<(Entity,&Window)>().iter(world).next().map(|(entity,window)|json!({"width":window.width(),"height":window.height(),"scale_factor":window.scale_factor(),"focused":window.focused,"visible":window.visible,"occluded":crate::control::window_occluded(world,entity)}));
    let tick = world.resource::<Clock>().ticks;
    let simulation_epoch = world.resource::<CreatureSim>().epoch;
    let simulation_tick_number = world
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .map(Simulation::tick_number);
    let simulation_local_tick = world
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .map(|_| world.resource::<CreatureSim>().sim_ticks);
    let simulation_target_tick = tick.saturating_sub(simulation_epoch);
    let simulation_caught_up = simulation_local_tick
        .is_none_or(|local| local == simulation_target_tick)
        && world
            .get_resource::<kn_render::DebugReplay>()
            .is_none_or(|replay| replay.caught_up);
    let simulation_tick = simulation_local_tick.map(|local| {
        if local == 0 && tick < simulation_epoch {
            tick
        } else {
            simulation_epoch + local
        }
    });
    let simulation_available = world.resource::<CreatureSim>().sim.is_some();
    let simulation_error = world.resource::<CreatureSim>().error.clone();
    let (render_frame, render_ready, render_failed) = world
        .get_resource::<crate::readiness::RenderReadiness>()
        .map_or(
            (
                world
                    .get_resource::<bevy::diagnostic::FrameCount>()
                    .map_or(0, |frame| frame.0),
                true,
                false,
            ),
            |readiness| readiness.observed(),
        );
    let preview_tick = world
        .get_resource::<ScrubState>()
        .filter(|scrub| scrub.active)
        .map(|scrub| scrub.day * CYCLE_TICKS + scrub.target);
    let phase = if preview_tick.is_some() {
        "lighting_preview"
    } else if error.is_some() {
        "error"
    } else if world.resource::<Rebuild>().map || requested != displayed {
        "loading_map"
    } else if simulation_error.is_some() {
        "simulation_failed"
    } else if !simulation_caught_up {
        "seeking"
    } else if render_failed {
        "render_failed"
    } else if !render_ready {
        "rendering"
    } else {
        "ready"
    };
    let (debug_player, debug) = crate::debug::projection(world);
    json!({"debug_player":debug_player,"debug":debug,"window":window,"viewport":viewport,"selected_render_layers":render_layers,"diagnostics":{"pieces_built":built,"pieces_visible":visible,"fps":fps,"creature_summary":creature_summary},"workspace":workspace_state,"requested_map": requested,"displayed_map":displayed,"error":error,"ready":displayed.is_some() && phase == "ready","phase":phase,"preview_tick":preview_tick,"render_frame":render_frame,"render_ready":render_ready,"render_failed":render_failed,"simulation_tick":simulation_tick,"simulation_target_tick":simulation_target_tick,"simulation_caught_up":simulation_caught_up,"simulation_tick_number":simulation_tick_number,"simulation_epoch":simulation_epoch,"simulation_local_tick":simulation_local_tick,"simulation_available":simulation_available,"simulation_error":simulation_error,"map_facts":map_facts,"tick":tick,"frame":world.get_resource::<bevy::diagnostic::FrameCount>().map_or(0,|frame|frame.0),"time":game_time_line(Some(tick)),"playing":playback,"speed":world.resource::<Clock>().rate(),"constant_day":constant_day,"overlays":overlays,"mode":mode,"selected_creature":selected,"hovered_creature":hovered,"creatures":creatures,"camera":camera,"camera_scale":world.resource::<CameraState>().scale(),"zoom":world.resource::<CameraState>().zoom})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Viewer {
            maps: vec![MapRef {
                source: MapSource::Game,
                number: 1,
            }],
            selected: 0,
            overlays: DEFAULT_OVERLAYS,
            playback: PlayState::Playing,
            constant_day: false,
        });
        world.insert_resource(ViewerWorkspace::default());
        world.insert_resource(Rebuild::default());
        world.insert_resource(Clock::new(TICK_MS as f32));
        world.insert_resource(CreatureSim::default());
        world
    }
    #[test]
    fn attack_projection_names_phase_target_and_refusal() {
        use kn_game::simulation::{AttackError, AttackOrder, AttackStatus};
        assert_eq!(
            attack_value(AttackOrder {
                target_id: 41,
                status: AttackStatus::Attacking
            }),
            json!({"target_id":41,"status":"attacking","error":null}),
        );
        assert_eq!(
            attack_value(AttackOrder {
                target_id: 41,
                status: AttackStatus::Refused(AttackError::NoRoute)
            }),
            json!({"target_id":41,"status":"refused","error":"no_route"}),
        );
    }

    #[test]
    fn invalid_requests_preserve_selection_and_map() {
        let mut world = world();
        world.resource_mut::<ViewerWorkspace>().selected_creature = Some(7);
        assert!(apply_action(&mut world, ViewerAction::Select { id: Some(999) }).is_err());
        assert_eq!(
            world.resource::<ViewerWorkspace>().selected_creature,
            Some(7)
        );
        assert!(
            apply_action(
                &mut world,
                ViewerAction::Map {
                    source: "game".into(),
                    number: 999
                }
            )
            .is_err()
        );
        assert_eq!(world.resource::<Viewer>().selected, 0);
        assert!(!world.resource::<Rebuild>().map);
    }
    #[test]
    fn escape_closes_context_and_keeps_selection() {
        let mut world = world();
        world.insert_resource(ViewerWorkspace {
            selected_creature: Some(7),
            mode: InteractionMode::Pan,
            time_open: true,
            display_open: true,
            diagnostics_open: true,
            ..default()
        });
        apply_action(&mut world, ViewerAction::Escape {}).unwrap();
        let workspace = world.resource::<ViewerWorkspace>();
        assert_eq!(workspace.mode, InteractionMode::Select);
        assert_eq!(workspace.selected_creature, Some(7));
        assert!(!workspace.time_open && !workspace.display_open && !workspace.diagnostics_open);
    }
    #[test]
    fn step_pauses_and_seek_uses_absolute_tick() {
        let mut world = world();
        apply_action(&mut world, ViewerAction::Seek { tick: 120 }).unwrap();
        apply_action(&mut world, ViewerAction::Step { ticks: -1 }).unwrap();
        assert_eq!(world.resource::<Clock>().ticks, 119);
        assert_eq!(world.resource::<Viewer>().playback, PlayState::Paused);
        apply_action(
            &mut world,
            ViewerAction::Display {
                layer: "lights".into(),
                visible: true,
            },
        )
        .unwrap();
        assert!(world.resource::<Viewer>().is_on(Overlay::Lights));
        assert!(!world.resource::<Rebuild>().map);
    }
    #[test]
    fn window_action_resizes_without_changing_camera_or_accepting_invalid_sizes() {
        let mut world = world();
        let window = world.spawn(Window::default()).id();
        world.insert_resource(CameraState { zoom: 4 });
        let camera = world
            .spawn((MapCamera, Transform::from_xyz(120.0, 340.0, 0.0)))
            .id();
        assert!(
            apply_action(
                &mut world,
                ViewerAction::Window {
                    width: 0,
                    height: 640
                }
            )
            .is_err()
        );
        assert!(
            apply_action(
                &mut world,
                ViewerAction::Window {
                    width: u32::MAX,
                    height: 640
                }
            )
            .is_err()
        );
        apply_action(
            &mut world,
            ViewerAction::Window {
                width: 960,
                height: 640,
            },
        )
        .unwrap();
        let window = world.entity(window).get::<Window>().unwrap();
        assert_eq!((window.width(), window.height()), (960.0, 640.0));
        assert_eq!(world.resource::<CameraState>().zoom, 4);
        assert_eq!(
            world.entity(camera).get::<Transform>().unwrap().translation,
            Vec3::new(120.0, 340.0, 0.0)
        );
    }

    #[test]
    fn seeks_and_steps_settle_after_the_playing_clocks_frame_advance() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AnimPlugin::new(60.0)))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(120),
            ))
            .insert_resource(Viewer {
                maps: vec![MapRef {
                    source: MapSource::Game,
                    number: 1,
                }],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: PlayState::Playing,
                constant_day: false,
            })
            .insert_resource(Stats {
                map: Some(MapRef {
                    source: MapSource::Game,
                    number: 1,
                }),
                ..default()
            })
            .init_resource::<CreatureSim>()
            .init_resource::<CameraState>()
            .init_resource::<Rebuild>()
            .add_systems(Update, apply_playback.after(AnimSet::Clock));
        install(&mut app);
        app.update();
        app.world_mut()
            .resource_mut::<ActionQueue>()
            .0
            .push_back(ViewerAction::Seek { tick: 100 });
        app.update();
        assert_eq!(app.world().resource::<Clock>().ticks, 100);
        assert_eq!(app.world().resource::<Clock>().state, PlayState::Paused);
        app.world_mut()
            .resource_mut::<ActionQueue>()
            .0
            .push_back(ViewerAction::Step { ticks: 1 });
        app.update();
        assert_eq!(app.world().resource::<Clock>().ticks, 101);
    }

    #[test]
    fn allowed_speed_controls_the_same_clock_and_survives_pause_and_seek() {
        let mut world = world();
        for rate in [0.25, 0.5, 1.0, 2.0, 4.0, 8.0] {
            apply_action(&mut world, ViewerAction::Speed { rate }).unwrap();
            assert_eq!(world.resource::<Clock>().rate(), rate);
        }
        assert!(apply_action(&mut world, ViewerAction::Speed { rate: 3.0 }).is_err());
        apply_action(&mut world, ViewerAction::Playback { playing: false }).unwrap();
        apply_action(&mut world, ViewerAction::Seek { tick: 100 }).unwrap();
        assert_eq!(world.resource::<Clock>().rate(), 8.0);
    }
    #[test]
    fn typed_requests_reject_missing_and_extra_fields() {
        assert!(serde_json::from_value::<ViewerAction>(json!({"type":"select"})).is_err());
        assert!(
            serde_json::from_value::<ViewerAction>(json!({"type":"state","surprise":1})).is_err()
        );
        assert!(
            serde_json::from_value::<ViewerAction>(json!({"type":"mode","mode":"combat"})).is_err()
        );
    }
    #[test]
    fn reset_positions_at_a_nonzero_clock_is_ready_on_the_same_tick() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1/rus/game");
        if !path.is_dir() {
            return;
        }
        let game = Game::open(path).unwrap();
        let map = MapRef {
            source: MapSource::Game,
            number: 1,
        };
        let mut simulation = game.simulation(map, SIM_SEED).unwrap();
        for _ in 0..100 {
            simulation.tick();
        }
        let mut world = world();
        world.insert_resource(GameResource(game));
        world.insert_resource(Stats {
            map: Some(map),
            ..default()
        });
        world.insert_resource(CameraState::default());
        world.insert_resource(CreatureSim {
            sim: Some(simulation),
            sim_ticks: 100,
            ..default()
        });
        world.resource_mut::<Clock>().ticks = 100;
        apply_action(&mut world, ViewerAction::ResetPositions {}).unwrap();
        let state = state_value(&mut world);
        assert_eq!(state["simulation_tick"], 100);
        assert_eq!(state["simulation_epoch"], 100);
        assert_eq!(state["simulation_local_tick"], 0);
        assert_eq!(state["simulation_tick_number"], 100);
        assert_eq!(state["ready"], true);
        apply_action(&mut world, ViewerAction::Step { ticks: -1 }).unwrap();
        let state = state_value(&mut world);
        assert_eq!(state["tick"], 99);
        assert_eq!(state["simulation_tick"], 99);
        assert_eq!(state["simulation_target_tick"], 0);
        assert_eq!(state["simulation_local_tick"], 0);
        assert_eq!(state["ready"], true);
    }

    #[test]
    #[ignore = "Opt-in projection cost measurement with local game data"]
    fn measure_workspace_projection_cost() {
        let path = std::env::var_os("GAME_DIR").expect("Set GAME_DIR to your local edition");
        let game = Game::open(PathBuf::from(path)).unwrap();
        let number = std::env::var("MAP")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(17);
        let map = MapRef {
            source: MapSource::Game,
            number,
        };
        let scene = game.scene(map, &SceneOptions::ALL).unwrap();
        let simulation = game.simulation(map, SIM_SEED).unwrap();
        let count = simulation.creature_count();
        let selected = simulation.states().first().map(|state| state.id);
        let mut world = world();
        world.resource_mut::<Viewer>().maps = vec![map];
        world.insert_resource(Stats {
            map: Some(map),
            ..default()
        });
        world.insert_resource(CameraState::default());
        world.insert_resource(CreatureSim {
            sim: Some(simulation),
            ..default()
        });
        world.resource_mut::<ViewerWorkspace>().selected_creature = selected;
        for _ in &scene.pieces {
            world.spawn((ScenePiece, InheritedVisibility::VISIBLE));
        }
        if let Some(creature_id) = selected {
            for layer in 0..8 {
                world.spawn((
                    CreatureSprite {
                        creature_id,
                        layer,
                        is_shadow: false,
                    },
                    InheritedVisibility::VISIBLE,
                ));
            }
        }
        for _ in 0..100 {
            std::hint::black_box(state_value(&mut world));
        }
        let mut samples = Vec::new();
        for _ in 0..2000 {
            let started = std::time::Instant::now();
            std::hint::black_box(state_value(&mut world));
            samples.push(started.elapsed().as_secs_f64() * 1e6);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "PROJECTION map={number} creatures={count} pieces={} selected_layers=8 samples={} median_us={:.3} p95_us={:.3}",
            scene.pieces.len(),
            samples.len(),
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100]
        );
    }
}
