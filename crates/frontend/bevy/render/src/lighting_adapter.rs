//! Game lighting inputs adapted to Bevy resources.

use bevy::prelude::*;
use kn_game::{Game, LightMask, LightingParams};

use kn_lighting::classic::LightingState;

pub fn load_light_masks(game: &Game) -> Vec<LightMask> {
    match game.light_masks() {
        Ok(masks) => masks,
        Err(error) => {
            warn!("{error}");
            Vec::new()
        }
    }
}

pub fn lighting_state(params: LightingParams) -> LightingState {
    LightingState {
        terrain_blend: Vec3::from(params.ambient),
        ambient_blend: Vec3::from(params.ambient),
        light_color: Vec3::from(params.ground_light),
        light_gain: if params.ground_window { 1.0 } else { 0.0 },
    }
}
