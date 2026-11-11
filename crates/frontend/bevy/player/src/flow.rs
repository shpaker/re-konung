//! The AppFlow adapter: the `kn-app` reducer wired into Bevy's `Update`.
//!
//! One frame of the pipeline, in order:
//!
//! 1. [`app_flush`] — drains the controller's action queue into
//!    [`PendingEffects`];
//! 2. [`apply_effects`] — executes every effect exactly once (session build,
//!    settings apply-and-persist, exit) and leaves result actions in the queue;
//! 3. [`app_flush`] again — the very same function a second time, reducing the
//!    result actions. They never emit effects (the reducer's guarantee), so a
//!    third round cannot be needed — a loop would only re-state that invariant;
//! 4. [`sync_screens`] — moves the screen root when the screen changed.
//!
//! [`session_watch`] sits outside the chain, ordered after the creature
//! pipeline and before it: it reads what the plugins did to the world this
//! frame and reports through the same frame's flush, so `Succeeded` lands on
//! the first frame the session is observably ready — never earlier.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, VideoModeSelection, WindowMode};
use kn_app::{
    Action, AppController, AppState, Effect, LoadingAction, PersistOutcome, SaveError, Screen,
    SessionError,
};
use kn_render::{CreatureMotion, CreatureSet, CreatureSim, MapRoot};
use kn_settings::{self, Settings, VideoSettings};

use crate::session::{self, SessionInit};

/// The `kn-app` controller as a resource: the only path from an [`Action`] to
/// an [`AppState`] change. The field is public for tests that position the
/// controller directly; the pipeline itself only ever calls [`Self::submit`].
#[derive(Resource)]
pub struct AppFlow(pub AppController);

impl AppFlow {
    pub fn new(applied: Settings) -> Self {
        Self(AppController::new(applied))
    }

    pub fn state(&self) -> &AppState {
        self.0.state()
    }

    /// Enqueues an action for the next [`app_flush`].
    pub fn submit(&mut self, action: Action) {
        self.0.submit(action);
    }
}

/// Effects drained from the reducer, waiting for [`apply_effects`]. Empty
/// except between the two systems of one frame.
#[derive(Resource, Default)]
pub struct PendingEffects(pub Vec<Effect>);

/// The runtime mirror of the applied settings: what the interface renders in
/// (language) and what a failed persist rolls the window back to. Advances
/// only when a save has landed, exactly like the reducer's `applied`.
#[derive(Resource)]
pub struct AppliedSettings(pub Settings);

/// Where settings persist. `None` — saving fails with a typed error instead of
/// writing somewhere random (mirrors `kn_settings::default_path`).
#[derive(Resource, Default)]
pub struct SettingsPath(pub Option<PathBuf>);

/// The last screen whose root was spawned; [`sync_screens`] despawns it on
/// change.
#[derive(Resource, Default)]
struct LastScreen(Option<Screen>);

/// Marks the root of the current screen's view: [`sync_screens`] despawns
/// the marked tree when the screen changes, and every view in
/// [`crate::screens`] composes this marker onto its root.
#[derive(Component, Clone, Default)]
pub(crate) struct ScreenRoot;

/// The flush→apply→flush→screens chain of one frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowSet;

/// Adds the AppFlow pipeline with its resources. The app must already run the
/// creature subsystem (`kn_render::CreatureAnimationPlugin`) and provide
/// [`kn_render::GameResource`], [`session::NewGame`], a [`kn_render::TextureCache`]
/// and `Assets<Image>`/`Assets<KonungLightMaterial>` — what any window drawing
/// this game has anyway.
pub struct PlayerFlowPlugin {
    /// The applied-and-persisted settings the startup loaded.
    pub settings: Settings,
    /// Where [`Effect::ApplyAndPersist`] writes; `None` disables saving.
    pub settings_path: Option<PathBuf>,
}

impl Plugin for PlayerFlowPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AppFlow::new(self.settings))
            .insert_resource(PendingEffects::default())
            .insert_resource(AppliedSettings(self.settings))
            .insert_resource(SettingsPath(self.settings_path.clone()))
            .init_resource::<LastScreen>()
            .configure_sets(Update, FlowSet)
            .add_systems(
                Update,
                (app_flush, apply_effects, app_flush, sync_screens)
                    .chain()
                    .in_set(FlowSet),
            )
            .add_systems(
                Update,
                // The settings widgets mirror the draft after the chain has
                // reduced this frame's actions into it.
                crate::screens::settings::sync.after(FlowSet),
            )
            .add_systems(
                Update,
                // Not in the chain: the watcher reads world state the plugins
                // changed earlier in the frame, so it runs after the creature
                // pipeline has spoken — and before the chain, so its report is
                // reduced by the same frame's flush instead of the next one.
                session_watch.after(CreatureSet::Nodes).before(FlowSet),
            );
    }
}

/// Drains the controller's queue into [`PendingEffects`]. Added twice in the
/// chain (before and after the executor); the reducer guarantees result
/// actions emit no effects, so the second drain never re-arms the executor.
fn app_flush(world: &mut World) {
    let effects = world.resource_mut::<AppFlow>().0.flush();
    world.resource_mut::<PendingEffects>().0.extend(effects);
}

/// Executes every pending effect exactly once: [`PendingEffects`] is the only
/// queue and it is taken empty. Failures are logged with their io detail and
/// path here, before the typed result action is submitted — the app state
/// carries kinds only.
fn apply_effects(world: &mut World) {
    let effects = std::mem::take(&mut world.resource_mut::<PendingEffects>().0);
    for effect in effects {
        match effect {
            Effect::StartSession => match session::init(world) {
                Ok(()) => {}
                Err(kind) => {
                    // init already tore its half-built session down; the
                    // report goes back the same frame.
                    world
                        .resource_mut::<AppFlow>()
                        .submit(Action::Loading(LoadingAction::Failed(kind)));
                }
            },
            Effect::ApplyAndPersist(new) => apply_and_persist(world, new),
            Effect::ExitApp => {
                world.write_message(AppExit::Success);
            }
        }
    }
}

/// Applies the draft to the runtime, persists it, and reports. On any failure
/// the runtime is rolled back to what the reducer still holds as applied —
/// the window and the language must not run ahead of a save that did not land.
fn apply_and_persist(world: &mut World, new: Settings) {
    apply_video(world, new.video);
    world.resource_mut::<AppliedSettings>().0 = new;
    let outcome = match world.resource::<SettingsPath>().0.as_deref() {
        Some(path) => match kn_settings::save(path, &new) {
            Ok(()) => None,
            Err(error) => {
                let (kind, detail) = match error {
                    kn_settings::SaveError::Write(io) => (SaveError::Write, io),
                    kn_settings::SaveError::Replace(io) => (SaveError::Replace, io),
                };
                error!("settings save to {} failed: {detail}", path.display());
                Some(PersistOutcome::SaveFailed(kind))
            }
        },
        None => {
            // Shaped as Write: nothing was ever written because there is
            // nowhere to write.
            error!("settings cannot be saved: no settings path was resolved at startup");
            Some(PersistOutcome::SaveFailed(SaveError::Write))
        }
    };
    match outcome {
        None => world
            .resource_mut::<AppFlow>()
            .submit(Action::Persisted(PersistOutcome::Saved)),
        Some(failed) => {
            let applied = world.resource::<AppFlow>().state().applied;
            apply_video(world, applied.video);
            world.resource_mut::<AppliedSettings>().0 = applied;
            world
                .resource_mut::<AppFlow>()
                .submit(Action::Persisted(failed));
        }
    }
}

/// Window-only application of the video settings; the language lives in
/// [`AppliedSettings`]. Headless worlds without a window are skipped.
fn apply_video(world: &mut World, video: VideoSettings) {
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    if let Ok(mut window) = windows.single_mut(world) {
        window.mode = if video.fullscreen {
            WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
        } else {
            WindowMode::Windowed
        };
        window.present_mode = if video.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
    }
}

/// Moves the screen root when the screen changed: the old root dies with its
/// whole tree, the new screen's view spawns fresh from the current state.
/// `Playing` has no root — the world is the view.
fn sync_screens(world: &mut World) {
    let screen = world.resource::<AppFlow>().state().screen;
    {
        let mut last = world.resource_mut::<LastScreen>();
        if last.0 == Some(screen) {
            return;
        }
        last.0 = Some(screen);
    }
    let mut roots = world.query_filtered::<Entity, With<ScreenRoot>>();
    let old: Vec<Entity> = roots.iter(world).collect();
    for root in old {
        world.despawn(root);
    }
    match screen {
        Screen::MainMenu => crate::screens::main_menu::spawn(world),
        Screen::Settings => crate::screens::settings::spawn(world),
        Screen::Loading => crate::screens::loading::spawn(world),
        Screen::Playing => {}
    }
}

/// Reports the session build's outcome once it is observable: `Succeeded` when
/// the simulation exists, the creature request is consumed and the map tree is
/// in the world; `Failed(Simulation)` when the request is gone and no
/// simulation appeared (the build died inside the creature subsystem). All
/// other failure kinds are reported by [`apply_effects`] from `session::init`.
fn session_watch(world: &mut World) {
    if !world.contains_resource::<SessionInit>() {
        return;
    }
    let sim_ready = world.resource::<CreatureSim>().sim.is_some();
    let request_pending = {
        let motion = world.resource::<CreatureMotion>();
        motion.rebuild || motion.scene.is_some()
    };
    if sim_ready && !request_pending {
        let mut roots = world.query_filtered::<(), With<MapRoot>>();
        if roots.iter(world).next().is_none() {
            // The framing has not landed yet; next frame.
            return;
        }
        world.remove_resource::<SessionInit>();
        world
            .resource_mut::<AppFlow>()
            .submit(Action::Loading(LoadingAction::Succeeded));
        return;
    }
    if !sim_ready && !request_pending {
        let reason = world
            .resource::<CreatureSim>()
            .error
            .clone()
            .unwrap_or_else(|| "unknown reason".to_string());
        error!("session simulation failed: {reason}");
        session::teardown(world);
        world
            .resource_mut::<AppFlow>()
            .submit(Action::Loading(LoadingAction::Failed(
                SessionError::Simulation,
            )));
    }
}

/// Run condition for the player's camera bindings: the map is panned and
/// zoomed only while the game screen is up, never over a menu.
pub fn screen_is_playing(flow: Res<AppFlow>) -> bool {
    flow.state().screen == Screen::Playing
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe system that only runs when its conditions let it.
    #[derive(Resource, Default)]
    struct ProbeRan(bool);

    fn probe(mut probe: ResMut<ProbeRan>) {
        probe.0 = true;
    }

    fn app_at(screen: Screen) -> App {
        let mut flow = AppFlow::new(Settings::default());
        // Drive the controller through legal transitions only; the emitted
        // effects are the pipeline's business, not the condition's.
        match screen {
            Screen::MainMenu => {}
            Screen::Settings => flow.submit(Action::MainMenu(kn_app::MainMenuAction::OpenSettings)),
            Screen::Loading | Screen::Playing => {
                flow.submit(Action::MainMenu(kn_app::MainMenuAction::NewGame));
                if screen == Screen::Playing {
                    flow.submit(Action::Loading(LoadingAction::Succeeded));
                }
                flow.0.flush();
            }
        }
        flow.0.flush();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(flow)
            .init_resource::<ProbeRan>()
            .add_systems(Update, probe.run_if(screen_is_playing));
        app
    }

    #[test]
    fn camera_input_runs_only_on_the_playing_screen() {
        for screen in [Screen::MainMenu, Screen::Settings, Screen::Loading] {
            let mut app = app_at(screen);
            app.update();
            assert!(
                !app.world().resource::<ProbeRan>().0,
                "the camera must not take input on {screen:?}"
            );
        }
        let mut app = app_at(Screen::Playing);
        app.update();
        assert!(
            app.world().resource::<ProbeRan>().0,
            "the camera takes input while playing"
        );
    }
}
