//! Ambient lighting of the whole scene: the sprite material extension, the
//! time contract, the pool of materials and the per-surface rules.
//!
//! The fragment shader reproduces the original's per-channel blend (Slavik
//! `gfx.cpp:56-84`); the CPU reference it must agree with is
//! `k1_game::lighting::apply_channel`. The render layer stays K1-free: a
//! frontend hands over ready-made uniform values ([`LightingState`]), the raw
//! mask brightness bytes ([`LightingMaterials::new`]) and the map's fine light
//! grid ([`FineLightGrid`]) — exactly the numbers the original passed to its
//! shader, derived by the frontend from its own game layer.
//!
//! Surfaces and their confirmed rules (`k1_game::lighting::fades` is the
//! authoritative encoding; [`surface_lighting`] applies it to scene pieces):
//!
//! - terrain and decorations fade only inside the ground-light window — their
//!   material's blend is the ground ambient, zero outside the window, and a
//!   zero blend is a no-op in the shader;
//! - static objects fade with the plain ambient always, except self-lit
//!   pieces (`Flames.size() >= 2`) and flames, which never fade
//!   (`SetFade(false)`, game.cpp:1563-1580, game.cpp:1604);
//! - characters fade where their current cell's fine light value is 0, in and
//!   out of the ground window alike (game.cpp:1611-1626); the material is
//!   swapped on the creature's sprites as it moves ([`update_creature_lighting`]);
//! - shadows never fade (`SetFade(false)` before `DrawShadow`, game.cpp:1300,
//!   game.cpp:1348) and neither do the overlay groups.

use std::collections::HashMap;

use bevy::{
    asset::{AssetPath, RenderAssetUsages},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{MaterialExtension2d, SpriteMaterial, SpriteMaterialPlugin},
};
use kn_game::{Piece, PieceGroup};

use crate::creature_animation::{CreatureSet, CreatureSim, CreatureSprite};

/// The material's fragment shader, embedded next to this module. The path is
/// spelled out twice because `load_shader_library!` only takes a literal. The
/// name honours where the formula was first reproduced (terrain); the same
/// shader serves the plain ambient materials — a zero mask keeps its light
/// term off, leaving only the ambient blend.
const SHADER_PATH: &str = "shaders/terrain_light_material.wesl";

/// Per-sprite lighting: the ambient blend and the tile-light colour as
/// uniforms, the tile's LIGHTS.RES mask as a second texture.
///
/// The uniforms start at 20 so they do not collide with the sprite's own
/// material bindings (0-2). A mask brightness byte `v` becomes
/// `round(v / 100 * 255)`, so the shader's sampled `.r` is the original's
/// `lightning` (`v / 100`).
#[derive(AsBindGroup, Asset, Reflect, Clone)]
pub struct KonungLightMaterial {
    /// Ambient /100: negative darkens, positive fades toward white.
    #[uniform(20)]
    blend: Vec3,
    /// Tile-light colour /200, added toward white where the mask is bright.
    /// Unused where the mask is zero — every plain ambient sprite.
    #[uniform(20)]
    light_color: Vec3,
    /// LIGHTS.RES tile mask; brightness in `.r`. A sprite without a mask gets
    /// a 1×1 zero texture, so `lightning` stays 0 and the formula reduces to
    /// the plain ambient blend.
    #[texture(21)]
    #[sampler(22)]
    mask: Handle<Image>,
}

impl MaterialExtension2d for KonungLightMaterial {
    fn fragment_shader() -> Option<ShaderRef> {
        Some(ShaderRef::Path(
            AssetPath::from_path_buf(bevy::asset::embedded_path!(SHADER_PATH))
                .with_source("embedded"),
        ))
    }
}

impl KonungLightMaterial {
    /// The ambient blend the shader multiplies/adds with — read side for
    /// tests and diagnostics; writes go through [`update_lighting_uniforms`].
    pub fn blend(&self) -> Vec3 {
        self.blend
    }

    /// The tile-light colour the shader adds where a mask is bright.
    pub fn light_color(&self) -> Vec3 {
        self.light_color
    }
}

/// The lighting time contract, written by the frontend once per frame from its
/// own game layer (`k1_game::lighting::params`); [`KonungLightingPlugin`]'s
/// systems only read it.
///
/// All-zero is the safe default: a zero blend takes the shader's non-negative
/// branch and adds zero — every surface renders unlit until the frontend
/// speaks up.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LightingState {
    /// Ambient for terrain and decorations: the ground ambient inside the
    /// ground-light window, zero outside it — the shader no-ops there.
    pub terrain_blend: Vec3,
    /// Ambient for objects and characters: applied always, window or not.
    pub ambient_blend: Vec3,
    /// Tile-light colour /200, added toward white where a terrain mask is
    /// bright.
    pub light_color: Vec3,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            terrain_blend: Vec3::ZERO,
            ambient_blend: Vec3::ZERO,
            light_color: Vec3::ZERO,
        }
    }
}

/// The map's LIGHTS.N fine grid — the tile-light mask index per fine cell,
/// row-major — that decides whether a standing creature is in a lit patch.
///
/// `None` or empty: every cell reads dark, so every creature fades (a map
/// without `LIGHTS.N` has no lit patches at all). Only the zero-ness of a
/// value matters, per `k1_game::lighting::fades`.
#[derive(Resource, Default)]
pub struct FineLightGrid(pub Option<Vec<Vec<u8>>>);

impl FineLightGrid {
    /// The fine light value at a simulation cell `(column, row)`; 0 — dark.
    pub fn value(&self, cell: (u32, u32)) -> u8 {
        match &self.0 {
            Some(grid) => grid
                .get(cell.1 as usize)
                .and_then(|row| row.get(cell.0 as usize))
                .copied()
                .unwrap_or(0),
            None => 0,
        }
    }
}

/// Which lighting material a static scene piece carries, per the confirmed
/// surface rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLighting {
    /// Terrain with a LIGHTS.N tile mask: the masked material, uniforms gated
    /// by the ground window.
    TerrainMasked(usize),
    /// Terrain without a mask and decorations: the plain material with the
    /// same ground-window gating (a zero mask makes the formula uniform).
    TerrainPlain,
    /// Static objects: the plain material driven by the plain ambient, which
    /// applies outside the ground window too.
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
/// the object with `SetFade(false)` (game.cpp:1604), so it never carries a
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

/// Registers the light material, its embedded shader, the time-contract
/// resources and the two update systems.
///
/// The shader is embedded and loaded permanently (not on demand) so the WESL
/// imports from `bevy_sprite_render` resolve: the import resolver sees only
/// shader assets that are loaded. The material pool itself is built by the
/// frontend once the map's masks are read — [`LightingMaterials::new`] — and
/// every system here tolerates its absence: sprites simply keep no material,
/// which is exactly the unlit look.
pub struct KonungLightingPlugin;

impl Plugin for KonungLightingPlugin {
    fn build(&self, app: &mut App) {
        bevy::shader::load_shader_library!(app, "shaders/terrain_light_material.wesl");
        app.add_plugins(SpriteMaterialPlugin::<KonungLightMaterial>::default())
            .init_resource::<LightingState>()
            .init_resource::<FineLightGrid>()
            .add_systems(
                Update,
                (
                    update_lighting_uniforms,
                    // After the creature pipeline placed the nodes: a creature
                    // spawned this frame gets its material the same frame, and
                    // the cells it reads are this frame's.
                    update_creature_lighting.after(CreatureSet::Nodes),
                ),
            );
    }
}

/// Pool of light materials: one per mask for the terrain, plus the two plain
/// ones — terrain/decorations (ground-window blend) and objects/characters
/// (plain ambient). Handles are shared by every sprite of the same kind, so a
/// map costs 21 materials however many sprites it draws.
#[derive(Resource)]
pub struct LightingMaterials {
    terrain_plain: Handle<KonungLightMaterial>,
    ambient_plain: Handle<KonungLightMaterial>,
    masks: Vec<Handle<KonungLightMaterial>>,
}

impl LightingMaterials {
    /// Builds the pool from raw mask brightness bytes; one handle per mask,
    /// indexed by `light_mask_index` (0-based). `width`/`height` are the tile
    /// mask dimensions the bytes are laid out for. The uniforms start at zero —
    /// [`update_lighting_uniforms`] writes the current [`LightingState`] into
    /// every material.
    pub fn new(
        images: &mut Assets<Image>,
        materials: &mut Assets<KonungLightMaterial>,
        masks: &[(Vec<u8>, u32, u32)],
    ) -> Self {
        let mut material = |mask: Handle<Image>| {
            materials.add(KonungLightMaterial {
                blend: Vec3::ZERO,
                light_color: Vec3::ZERO,
                mask,
            })
        };
        let terrain_plain = material(images.add(zero_mask_image()));
        let ambient_plain = material(images.add(zero_mask_image()));
        let masks = masks
            .iter()
            .map(|(mask, width, height)| {
                material(images.add(light_mask_image(mask, *width, *height)))
            })
            .collect();
        Self {
            terrain_plain,
            ambient_plain,
            masks,
        }
    }

    /// The material for a terrain piece with this mask index; `None` (or an
    /// index past the pool) falls back to the plain terrain material.
    pub fn terrain(&self, mask_index: Option<usize>) -> Handle<KonungLightMaterial> {
        mask_index
            .and_then(|index| self.masks.get(index))
            .unwrap_or(&self.terrain_plain)
            .clone()
    }

    /// The material for surfaces the plain ambient drives: objects (except
    /// self-lit and flames) and characters in dark cells.
    pub fn ambient(&self) -> Handle<KonungLightMaterial> {
        self.ambient_plain.clone()
    }

    /// How many materials the pool holds: the masks plus the two plain ones.
    pub fn len(&self) -> usize {
        self.masks.len() + 2
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Writes the current [`LightingState`] into every material of the pool — the
/// single place the render layer touches the uniforms, so both frontends get
/// identical behaviour by inserting their state into one resource.
///
/// Runs only when the state changed: a rewritten asset is re-extracted for the
/// GPU, and lighting shifts a few times a minute, not every frame.
pub fn update_lighting_uniforms(
    state: Res<LightingState>,
    pool: Option<Res<LightingMaterials>>,
    mut materials: ResMut<Assets<KonungLightMaterial>>,
) {
    let Some(pool) = pool else {
        return;
    };
    if !state.is_changed() {
        return;
    }
    let mut write = |handle: &Handle<KonungLightMaterial>, blend: Vec3| {
        if let Some(mut material) = materials.get_mut(handle) {
            material.blend = blend;
            material.light_color = state.light_color;
        }
    };
    for handle in pool
        .masks
        .iter()
        .chain(std::iter::once(&pool.terrain_plain))
    {
        write(handle, state.terrain_blend);
    }
    write(&pool.ambient_plain, state.ambient_blend);
}

/// Swaps the lighting material of every creature sprite as its cell lights up
/// or goes dark.
///
/// A creature in a dark cell (fine value 0 — the whole grid when the map has
/// none) fades with the plain ambient like an object, in and out of the
/// ground window alike; in a lit cell it is drawn without a material
/// (game.cpp:1611-1626). Only the material component moves: the node, its
/// transform, depth and animator are untouched, so a light change can never
/// respawn or re-anchor a creature.
///
/// The cells come straight from [`CreatureSim`] — the same simulation the
/// movement systems read — so there is no second source of creature positions
/// to keep in sync. Shadow sprites are skipped: the original draws them with
/// `SetFade(false)` (game.cpp:1300, game.cpp:1348).
#[allow(clippy::type_complexity)]
pub fn update_creature_lighting(
    mut commands: Commands,
    sim: Option<Res<CreatureSim>>,
    grid: Option<Res<FineLightGrid>>,
    pool: Option<Res<LightingMaterials>>,
    sprites: Query<(
        Entity,
        &CreatureSprite,
        Has<SpriteMaterial<KonungLightMaterial>>,
    )>,
) {
    let (Some(sim), Some(grid), Some(pool)) = (sim, grid, pool) else {
        return;
    };
    let Some(ref simulation) = sim.sim else {
        return;
    };
    // Dark per creature id: one lookup per sprite instead of one per cell read.
    let dark: HashMap<usize, bool> = simulation
        .states()
        .iter()
        .map(|state| (state.id, grid.value(state.cell) == 0))
        .collect();
    for (entity, tag, has_material) in &sprites {
        if tag.is_shadow {
            continue;
        }
        let Some(&is_dark) = dark.get(&tag.creature_id) else {
            continue;
        };
        match (is_dark, has_material) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert(SpriteMaterial(pool.ambient()));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<SpriteMaterial<KonungLightMaterial>>();
            }
            _ => {}
        }
    }
}

/// Mask brightness bytes → an `R8Unorm` image of `width × height`, one byte
/// per pixel scaled `round(v / 100 * 255)` so the sampled `.r` reads back
/// `v / 100`. The row-major bytes map onto the sprite through the same UV as
/// the tile texture.
pub fn light_mask_image(mask: &[u8], width: u32, height: u32) -> Image {
    let data = mask
        .iter()
        .map(|&value| ((u16::from(value) * 255 + 50) / 100) as u8)
        .collect();
    mask_image(data, width, height)
}

/// A 1×1 zero mask: `lightning` samples to 0 everywhere.
fn zero_mask_image() -> Image {
    mask_image(vec![0], 1, 1)
}

fn mask_image(data: Vec<u8>, width: u32, height: u32) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The reference draws the ground with a linear filter (SetLinearFilter).
    image.sampler = bevy::image::ImageSampler::linear();
    image
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
    fn mask_brightness_scales_by_100() {
        // 43 is the largest brightness in shipped LIGHTS.RES: 43/100 of white.
        assert_eq!(
            light_mask_image(&[0, 43, 100], 3, 1).data,
            Some(vec![0, 110, 255])
        );
        // Rounding: 1 * 2.55 = 2.55 → 3, matching round(v / 100 * 255).
        assert_eq!(light_mask_image(&[1], 1, 1).data, Some(vec![3]));
    }

    #[test]
    fn mask_image_is_single_channel_and_linear() {
        let image = light_mask_image(&vec![7; 114 * 64], 114, 64);
        assert_eq!(image.size(), UVec2::new(114, 64));
        assert_eq!(image.texture_descriptor.format, TextureFormat::R8Unorm);
        assert!(matches!(
            image.sampler,
            bevy::image::ImageSampler::Descriptor(_)
        ));
    }

    #[test]
    fn pool_serves_two_plain_materials_and_one_per_mask() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let pool = LightingMaterials::new(
            &mut images,
            &mut materials,
            &[(vec![1u8; 4], 2, 2), (vec![2u8; 4], 2, 2)],
        );
        assert_eq!(pool.len(), 4, "two masks plus the two plain materials");
        assert_ne!(pool.terrain(Some(0)), pool.terrain(Some(1)));
        assert_ne!(pool.terrain(None), pool.terrain(Some(0)));
        assert_ne!(pool.ambient(), pool.terrain(None));
        // An index past the pool falls back to the plain terrain material
        // instead of panicking: mask indices come from the data.
        assert_eq!(pool.terrain(Some(2)), pool.terrain(None));
    }

    #[test]
    fn the_full_pool_is_nineteen_masks_plus_two_plains() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        // LIGHTS.RES ships exactly 19 tile masks; the pool is that plus the
        // two plain materials.
        let masks: Vec<(Vec<u8>, u32, u32)> = (0..19).map(|_| (vec![0u8], 1, 1)).collect();
        let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
        assert_eq!(pool.len(), 21, "19 LIGHTS.RES masks + terrain + ambient");
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
        // Decorations share the terrain gating, never a mask.
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
    fn the_default_state_lights_nothing() {
        let state = LightingState::default();
        assert_eq!(
            state,
            LightingState {
                terrain_blend: Vec3::ZERO,
                ambient_blend: Vec3::ZERO,
                light_color: Vec3::ZERO,
            }
        );
    }

    #[test]
    fn the_fine_grid_reads_cells_and_treats_missing_data_as_dark() {
        let grid = FineLightGrid(Some(vec![vec![0, 5], vec![7, 0]]));
        assert_eq!(grid.value((1, 0)), 5);
        assert_eq!(grid.value((0, 1)), 7);
        assert_eq!(grid.value((0, 0)), 0);
        // Off-grid cells are dark, like the map edge the original pads with 0.
        assert_eq!(grid.value((9, 9)), 0);
        assert_eq!(FineLightGrid(None).value((0, 0)), 0);
        assert_eq!(FineLightGrid(Some(Vec::new())).value((0, 0)), 0);
    }

    /// The uniform writer drives every material of the pool from one state:
    /// terrain materials take the ground blend, the ambient one the plain
    /// blend, and a second state rewrites them all.
    #[test]
    fn lighting_state_reaches_every_material_of_the_pool() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let masks: Vec<(Vec<u8>, u32, u32)> = (0..3).map(|_| (vec![1u8; 4], 2, 2)).collect();
        let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
        let handles: Vec<Handle<KonungLightMaterial>> = (0..3)
            .map(|index| pool.terrain(Some(index)))
            .chain([pool.terrain(None), pool.ambient()])
            .collect();
        let mut app = App::new();
        // The writer only touches the asset stores; no AssetPlugin needed.
        app.insert_resource(images);
        app.insert_resource(materials);
        app.insert_resource(pool);
        app.insert_resource(LightingState {
            terrain_blend: Vec3::new(-0.6, -0.5, -0.7),
            ambient_blend: Vec3::new(-0.3, -0.2, -0.4),
            light_color: Vec3::new(0.1, 0.05, 0.0),
        });
        app.add_systems(Update, update_lighting_uniforms);

        app.update();
        {
            let materials = app.world().resource::<Assets<KonungLightMaterial>>();
            for (index, handle) in handles.iter().enumerate() {
                let material = materials.get(handle).expect("pool material alive");
                let want = if index == handles.len() - 1 {
                    Vec3::new(-0.3, -0.2, -0.4)
                } else {
                    Vec3::new(-0.6, -0.5, -0.7)
                };
                assert_eq!(material.blend, want, "material #{index}");
                assert_eq!(material.light_color, Vec3::new(0.1, 0.05, 0.0));
            }
        }

        // A new state — day, the ground window closed — rewrites them all, and
        // the terrain blends land exactly on zero, the shader's no-op.
        app.insert_resource(LightingState {
            terrain_blend: Vec3::ZERO,
            ambient_blend: Vec3::new(0.1, 0.05, 0.0),
            light_color: Vec3::ZERO,
        });
        app.update();
        let materials = app.world().resource::<Assets<KonungLightMaterial>>();
        for (index, handle) in handles.iter().enumerate() {
            let material = materials.get(handle).expect("pool material alive");
            let want = if index == handles.len() - 1 {
                Vec3::new(0.1, 0.05, 0.0)
            } else {
                Vec3::ZERO
            };
            assert_eq!(material.blend, want, "material #{index} after the change");
        }
    }

    /// Without the pool the system is a no-op, not a panic: a frontend may run
    /// a frame before it builds the masks.
    #[test]
    fn the_uniform_writer_tolerates_a_missing_pool() {
        let mut app = App::new();
        app.insert_resource(Assets::<KonungLightMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(LightingState::default())
            .add_systems(Update, update_lighting_uniforms);
        app.update();
    }
}
