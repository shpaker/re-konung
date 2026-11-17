//! Scene-surface policy and synchronization of creature lighting inputs.

use std::collections::HashMap;

use bevy::prelude::*;
use kn_game::{Piece, PieceGroup};
use kn_lighting::LightingSurface;
use kn_lighting::classic::{ClassicSurface, LightingMaterials};

use crate::creature_animation::{CreatureSim, CreatureSprite};

/// Which lighting material a static scene piece carries, per the confirmed
/// surface rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLighting {
    /// Terrain with a LIGHTS.N tile mask: the masked material. The mask's
    /// contribution is gated by the ground window (`light_gain`); the ambient
    /// blend follows the phase like every surface.
    TerrainMasked(usize),
    /// Terrain without a mask and decorations: the plain material — the phase
    /// ambient only (a zero mask makes the formula uniform).
    TerrainPlain,
    /// Static objects: the plain material with the same phase ambient.
    Ambient,
    /// No material: flames and self-lit objects are drawn with `SetFade(false)`,
    /// and so are shadows and every overlay group.
    None,
}

/// Maps a scene piece to its lighting surface. Pure and GPU-free: the decision
/// the confirmed rules encode, separated from sprite building so it can be
/// tested without a world.
///
/// `is_flame` is [`Piece::is_flame`] — an object's fire is drawn right after
/// the object with `SetFade(false)` (game.cpp:1592-1593), so it never carries a
/// material even though it sits in the Objects group.
pub fn surface_lighting(
    group: PieceGroup,
    light_mask_index: Option<usize>,
    self_lit: bool,
    is_flame: bool,
) -> SurfaceLighting {
    match group {
        PieceGroup::Terrain => match light_mask_index {
            Some(index) => SurfaceLighting::TerrainMasked(index),
            None => SurfaceLighting::TerrainPlain,
        },
        PieceGroup::Decorations => SurfaceLighting::TerrainPlain,
        PieceGroup::Objects if !(self_lit || is_flame) => SurfaceLighting::Ambient,
        _ => SurfaceLighting::None,
    }
}

/// The same mapping straight off a piece.
pub fn piece_surface_lighting(piece: &Piece) -> SurfaceLighting {
    surface_lighting(
        piece.group,
        piece.light_mask_index,
        piece.self_lit,
        piece.is_flame(),
    )
}

pub(crate) fn attach_piece_lighting(
    entity: &mut EntityCommands<'_>,
    piece: &Piece,
    pool: Option<&LightingMaterials>,
) {
    let surface = match piece_surface_lighting(piece) {
        SurfaceLighting::TerrainMasked(index) => ClassicSurface::Terrain {
            mask_index: Some(index),
        },
        SurfaceLighting::TerrainPlain => ClassicSurface::Terrain { mask_index: None },
        SurfaceLighting::Ambient => ClassicSurface::Ambient,
        SurfaceLighting::None => return,
    };
    entity.insert(LightingSurface);
    if let Some(pool) = pool {
        pool.attach(entity, surface);
    }
}

pub(crate) fn attach_creature_lighting(
    entity: &mut EntityCommands<'_>,
    pool: Option<&LightingMaterials>,
) {
    entity.insert(LightingSurface);
    if let Some(pool) = pool {
        pool.attach(entity, ClassicSurface::Moving { cell: None });
    }
}

pub fn update_creature_lighting(
    sim: Option<Res<CreatureSim>>,
    mut cells: Local<HashMap<usize, (u32, u32)>>,
    mut sprites: Query<(&CreatureSprite, &mut ClassicSurface)>,
) {
    cells.clear();
    if let Some(simulation) = sim.as_ref().and_then(|sim| sim.sim.as_ref()) {
        cells.extend(
            simulation
                .states()
                .iter()
                .map(|state| (state.id, state.cell)),
        );
    }
    for (tag, mut surface) in &mut sprites {
        if tag.is_shadow {
            continue;
        }
        let next = ClassicSurface::Moving {
            cell: cells.get(&tag.creature_id).copied(),
        };
        if *surface != next {
            *surface = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn piece(group: PieceGroup) -> Piece {
        Piece {
            texture: None,
            x: 0,
            y: 0,
            w: None,
            h: None,
            animation: None,
            group,
            creature_id: None,
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
        }
    }

    #[test]
    fn surfaces_map_to_their_confirmed_materials() {
        // Terrain: masked when the LIGHTS.N tile says so, plain otherwise.
        assert_eq!(
            surface_lighting(PieceGroup::Terrain, Some(3), false, false),
            SurfaceLighting::TerrainMasked(3)
        );
        assert_eq!(
            surface_lighting(PieceGroup::Terrain, None, false, false),
            SurfaceLighting::TerrainPlain
        );
        // Decorations take the plain terrain material, never a mask.
        assert_eq!(
            surface_lighting(PieceGroup::Decorations, Some(3), false, false),
            SurfaceLighting::TerrainPlain
        );
        // Objects fade with the plain ambient…
        assert_eq!(
            surface_lighting(PieceGroup::Objects, None, false, false),
            SurfaceLighting::Ambient
        );
        // …except self-lit ones and flames, drawn with SetFade(false).
        assert_eq!(
            surface_lighting(PieceGroup::Objects, None, true, false),
            SurfaceLighting::None,
            "self-lit object"
        );
        assert_eq!(
            surface_lighting(PieceGroup::Objects, None, false, true),
            SurfaceLighting::None,
            "flame"
        );
        // Shadows and every overlay group stay untouched.
        for group in [
            PieceGroup::Shadows,
            PieceGroup::Characters,
            PieceGroup::CharacterShadows,
            PieceGroup::Lights,
            PieceGroup::Foot,
            PieceGroup::Markers,
        ] {
            assert_eq!(
                surface_lighting(group, Some(1), false, false),
                SurfaceLighting::None,
                "{group:?}"
            );
        }
    }

    #[test]
    fn piece_surface_lighting_reads_the_flame_from_the_piece() {
        let mut object = piece(PieceGroup::Objects);
        assert_eq!(piece_surface_lighting(&object), SurfaceLighting::Ambient);
        object.self_lit = true;
        assert_eq!(piece_surface_lighting(&object), SurfaceLighting::None);
    }
    #[test]
    fn eligible_surfaces_are_marked_without_a_classic_pool() {
        use bevy::ecs::world::CommandQueue;
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut expected = Vec::new();
        {
            let mut commands = Commands::new(&mut queue, &world);
            for group in PieceGroup::ALL {
                let mut entity = commands.spawn_empty();
                attach_piece_lighting(&mut entity, &piece(group), None);
                expected.push((
                    entity.id(),
                    matches!(
                        group,
                        PieceGroup::Terrain | PieceGroup::Decorations | PieceGroup::Objects
                    ),
                ));
            }
            let mut creature = commands.spawn_empty();
            attach_creature_lighting(&mut creature, None);
            expected.push((creature.id(), true));
            let mut self_lit = piece(PieceGroup::Objects);
            self_lit.self_lit = true;
            let mut entity = commands.spawn_empty();
            attach_piece_lighting(&mut entity, &self_lit, None);
            expected.push((entity.id(), false));
        }
        queue.apply(&mut world);
        for (entity, marked) in expected {
            assert_eq!(world.get::<LightingSurface>(entity).is_some(), marked);
            assert!(world.get::<ClassicSurface>(entity).is_none());
        }
    }
}
