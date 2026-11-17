//! Bevy-free game facade: loading, sessions, scenes and validation.

mod check;
mod game;
mod inspection;
mod new_game;
mod scene;
pub mod session;
mod visual_assets;

pub mod debug;

pub use session::Simulation;

pub use check::{CheckReport, MapCheck};
pub use game::{Game, GameError, MapRef, MapSource, Result};
pub use new_game::{
    CLASS_KUPETC, CLASS_OHOTNIK, CLASS_VOIN, CLASS_VOJD, COMBAT_ACCURACY, COMBAT_FENCING,
    HeroPreview, HeroSummary, NewGameClass, NewGameCombatStyle, NewGameSecondarySet,
    SECONDARY_DEFAULT, SECONDARY_MEDICINE, SECONDARY_TRADING, newgame_classes, newgame_profile,
};
pub use scene::{
    Animation, AnimationFrame, AnimationStats, CreatureAnimation, CreatureClips, CreatureLayer,
    DecodedTexture, Piece, Scene, SceneOptions, TextureId, group_is_shown,
};

pub use k1_data::characters::LayerKey;
pub use k1_data::objects_res::TICK_MS;
pub use k1_game::geometry::{FINE_HEIGHT, FINE_WIDTH, fine_grid};
pub use k1_game::presentation::scene::{PieceGroup, PoseKind};
pub use k1_game::presentation::textures::{Rgba, TextureError, TextureKey};
pub use k1_game::rules::new_game::{
    Allocation as NewGameAllocation, ChangeBlocked as NewGameChangeBlocked,
    allocation as newgame_allocation,
};
pub use k1_game::rules::time_of_day::{DayPhase, GameTime};

pub use k1_game::presentation::lighting::{LightingParams, params_for_map as lighting_params};
pub use k1_game::rules::time_of_day::{CYCLE_TICKS, game_time};
pub use visual_assets::{LightMask, LightMaskError};

#[cfg(test)]
mod tests;
