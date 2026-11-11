//! Shared fixtures of the player's integration tests: the app the window
//! runs, minus the window.

use std::path::PathBuf;

use bevy::prelude::*;
use kn_fe_player::flow::PlayerFlowPlugin;
use kn_fe_player::session::NewGame;
use kn_game::{Game, MapSource, TICK_MS};
use kn_render::{
    AnimPlugin, Clock, CreatureAnimationPlugin, GameResource, KonungLightMaterial, PendingFrame,
    PlayState, TextureCache,
};

/// First edition directory with data, or `None` to skip.
pub(crate) fn game_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    for edition in ["rus", "eng"] {
        let dir = root.join(edition).join("game");
        if dir.is_dir() {
            return Some(dir);
        }
    }
    eprintln!("skip: no original/k1/{{rus,eng}}/game (docs/k1/editions.md)");
    None
}

/// The app the window runs, minus the window: the AppFlow pipeline over the
/// shared creature subsystem and its clock, with the resources any window
/// drawing this game provides anyway. The camera plugin is left out (there is
/// no window to frame), but `PendingFrame` — its resource — stays, because
/// the session build frames through it.
pub(crate) fn player_app(game: Game, hero: u32) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AnimPlugin::new(TICK_MS as f32),
        CreatureAnimationPlugin,
    ))
    .insert_resource(GameResource(game))
    .init_resource::<TextureCache>()
    .insert_resource(Assets::<Image>::default())
    .insert_resource(Assets::<KonungLightMaterial>::default())
    .init_resource::<PendingFrame>()
    .insert_resource(NewGame { hero })
    .add_plugins(PlayerFlowPlugin {
        settings: kn_settings::defaults(),
        settings_path: None,
    });
    // The clock moves only through the test: the subsystem has no clock of its
    // own and must stand still while the test is not stepping it.
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    app
}

/// The first hero of the starting state, which the window opens.
pub(crate) fn first_hero(game: &Game) -> u32 {
    game.heroes(MapSource::Game).unwrap()[0]
}
