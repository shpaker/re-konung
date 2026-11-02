//! Apply K1 creature [`Decision`]s through the movement [`World`].
//!
//! Import decisions from k1_game::rules and call this module to mutate the
//! world. Uses explicit `(CreatureId, Decision)` pairs so there is no index
//! mismatch between creatures and decisions.
//!
//! See docs/rust.md for "Evaluation vs application in a tick".

use super::movement::grid::Cell;
use super::movement::pace::Pace;
use super::movement::passage::Passage;
use super::movement::world::World;
use k1_game::rules::Decision;

/// Apply evaluated decisions through the world.
///
/// Takes explicit `(CreatureId, Decision)` pairs: this avoids index-out-of-
/// bounds when the decision count does not match the roster count. The caller
/// builds these pairs from their own identity mapping (roster index →
/// World-assigned CreatureId).
pub fn apply<P: Passage, K: Pace>(
    world: &mut World<P, K>,
    commands: &[(super::movement::CreatureId, Decision)],
) {
    for (creature_id, decision) in commands {
        let target = match decision {
            Decision::None => continue,
            Decision::MoveTo(tx, ty) => Cell::new(*tx, *ty),
        };
        if let Err(e) = world.move_to(*creature_id, target) {
            eprintln!("move_to failed for {creature_id:?}: {e:?}");
        }
    }
}
