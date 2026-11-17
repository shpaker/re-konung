//! Classic palette lighting, isolated from game formats and scene entities.

use bevy::{
    asset::{AssetPath, RenderAssetUsages},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{MaterialExtension2d, SpriteMaterial, SpriteMaterialPlugin},
};

use crate::{LightingSet, LightingSurface};

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
    /// Positive light factors, added toward white where the mask is bright.
    /// Unused where the mask is zero — every plain ambient sprite.
    #[uniform(20)]
    light_color: Vec3,
    /// The ground-light window gate: 1.0 while the window is open, 0.0 outside.
    /// Scales the sampled mask brightness, so a closed window leaves the mask
    /// inert — no light colour, no cancellation — while the ambient blend keeps
    /// following the phase for every surface (the original gates only the tile
    /// light, never the fade; rus 0x425354).
    #[uniform(20)]
    light_gain: f32,
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

    /// The ground-light window gate (1.0 open, 0.0 closed).
    pub fn light_gain(&self) -> f32 {
        self.light_gain
    }
}

/// The lighting time contract, written by the frontend once per frame from its
/// own game layer (`k1_game::presentation::lighting::params`); [`KonungLightingPlugin`]'s
/// systems only read it.
///
/// All-zero is the safe default: a zero blend takes the shader's non-negative
/// branch and adds zero — every surface renders unlit until the frontend
/// speaks up.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LightingState {
    /// Ambient for terrain and decorations: the phase value, window or not —
    /// the original fades the ground together with everything on it.
    pub terrain_blend: Vec3,
    /// Ambient for objects and characters: the same phase value.
    pub ambient_blend: Vec3,
    /// Positive light factors, added toward white where a terrain mask is
    /// bright.
    pub light_color: Vec3,
    /// The ground-light window gate for the tile light: 1.0 while the window
    /// (ticks 8100..18900) is open, 0.0 outside — the masks and the light
    /// colour switch with it, the ambient blend never does.
    pub light_gain: f32,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            terrain_blend: Vec3::ZERO,
            ambient_blend: Vec3::ZERO,
            light_color: Vec3::ZERO,
            light_gain: 0.0,
        }
    }
}

/// The map's LIGHTS.N fine grid — the tile-light mask index per fine cell,
/// row-major — that decides whether a standing creature is in a lit patch.
///
/// `None` or empty: every cell reads dark, so every creature fades (a map
/// without `LIGHTS.N` has no lit patches at all). Only the zero-ness of a
/// value matters, per `k1_game::presentation::lighting::fades`.
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

/// Registers the light material, its embedded shader, the time-contract
/// resources and the two update systems.
///
/// The shader is embedded and loaded permanently (not on demand) so the WESL
/// imports from `bevy_sprite_render` resolve: the import resolver sees only
/// shader assets that are loaded. The material pool itself is built by the
/// frontend once the map's masks are read — [`LightingMaterials::new`] — and
/// the pool must precede creature spawning. Without it, headless consumers
/// can still spawn ordinary unlit sprites, but cannot enable lighting later.
pub struct KonungLightingPlugin;

impl Plugin for KonungLightingPlugin {
    fn build(&self, app: &mut App) {
        bevy::shader::load_shader_library!(app, "shaders/terrain_light_material.wesl");
        app.add_plugins(SpriteMaterialPlugin::<KonungLightMaterial>::default())
            .init_resource::<LightingState>()
            .init_resource::<FineLightGrid>()
            .add_systems(Update, update_lighting_uniforms.in_set(LightingSet::Apply));
        add_classic_surface_system(app);
    }
}

/// Shared terrain masks, two ambient variants and an immutable neutral variant.
/// A full map costs 22 materials regardless of its sprite count.
#[derive(Resource)]
pub struct LightingMaterials {
    terrain_plain: Handle<KonungLightMaterial>,
    ambient_plain: Handle<KonungLightMaterial>,
    neutral: Handle<KonungLightMaterial>,
    masks: Vec<Handle<KonungLightMaterial>>,
}

impl LightingMaterials {
    /// Builds the pool from raw mask brightness bytes; one handle per mask,
    /// indexed by `light_mask_index` (0-based). `width`/`height` are the tile
    /// mask dimensions the bytes are laid out for. The uniforms start at zero —
    /// [`update_lighting_uniforms`] writes the current [`LightingState`] into
    /// the ambient and terrain materials; the neutral variant stays unchanged.
    pub fn new(
        images: &mut Assets<Image>,
        materials: &mut Assets<KonungLightMaterial>,
        masks: &[(Vec<u8>, u32, u32)],
    ) -> Self {
        let mut material = |mask: Handle<Image>| {
            materials.add(KonungLightMaterial {
                blend: Vec3::ZERO,
                light_color: Vec3::ZERO,
                light_gain: 0.0,
                mask,
            })
        };
        let terrain_plain = material(images.add(zero_mask_image()));
        let ambient_plain = material(images.add(zero_mask_image()));
        let neutral = material(images.add(zero_mask_image()));
        let masks = masks
            .iter()
            .map(|(mask, width, height)| {
                material(images.add(light_mask_image(mask, *width, *height)))
            })
            .collect();
        Self {
            terrain_plain,
            ambient_plain,
            neutral,
            masks,
        }
    }

    /// Installs a material at creation so it never changes pipeline type later.
    pub fn attach(&self, entity: &mut EntityCommands<'_>, surface: ClassicSurface) {
        let handle = match surface {
            ClassicSurface::Terrain { mask_index } => self.terrain(mask_index),
            ClassicSurface::Ambient => self.ambient(),
            ClassicSurface::Moving { .. } => self.neutral(),
        };
        entity.insert((LightingSurface, surface, SpriteMaterial(handle)));
        if matches!(surface, ClassicSurface::Moving { .. }) {
            entity.insert(MovingSurface);
        }
    }

    /// The material for a terrain piece with this mask index; `None` (or an
    /// index past the pool) falls back to the plain terrain material.
    fn terrain(&self, mask_index: Option<usize>) -> Handle<KonungLightMaterial> {
        mask_index
            .and_then(|index| self.masks.get(index))
            .unwrap_or(&self.terrain_plain)
            .clone()
    }

    /// The material for surfaces the plain ambient drives: objects (except
    /// self-lit and flames) and characters in dark cells.
    fn ambient(&self) -> Handle<KonungLightMaterial> {
        self.ambient_plain.clone()
    }

    /// Keeps lit creatures on the same pipeline as dark creatures.
    fn neutral(&self) -> Handle<KonungLightMaterial> {
        self.neutral.clone()
    }

    /// How many materials the pool holds, including the neutral variant.
    pub fn len(&self) -> usize {
        self.masks.len() + 3
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Writes [`LightingState`] into the ambient and terrain materials — the
/// single place the render layer touches the uniforms, so both frontends get
/// identical behaviour by inserting their state into one resource.
///
/// Runs when the state changed or the pool is new: a fresh pool starts with
/// zero uniforms and must be lit by the state that is already standing — a map
/// switch mid-phase would otherwise render unlit until the state moves (on a
/// plateau that is up to the next flicker block, ~a second).
pub fn update_lighting_uniforms(
    state: Res<LightingState>,
    pool: Option<Res<LightingMaterials>>,
    mut materials: ResMut<Assets<KonungLightMaterial>>,
) {
    let Some(pool) = pool else {
        return;
    };
    if !state.is_changed() && !pool.is_changed() {
        return;
    }
    let mut write = |handle: &Handle<KonungLightMaterial>, blend: Vec3| {
        if let Some(mut material) = materials.get_mut(handle) {
            material.blend = blend;
            material.light_color = state.light_color;
            material.light_gain = state.light_gain;
        }
    };
    // The neutral material is deliberately excluded: lit creatures never fade.
    for handle in pool
        .masks
        .iter()
        .chain(std::iter::once(&pool.terrain_plain))
    {
        write(handle, state.terrain_blend);
    }
    write(&pool.ambient_plain, state.ambient_blend);
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassicSurface {
    Terrain { mask_index: Option<usize> },
    Ambient,
    Moving { cell: Option<(u32, u32)> },
}

/// Installs a pool before scene and creature spawning.
pub fn install_classic_lighting(world: &mut World, masks: &[(Vec<u8>, u32, u32)]) {
    let pool = world.resource_scope::<Assets<Image>, _>(|world, mut images| {
        let mut materials = world.resource_mut::<Assets<KonungLightMaterial>>();
        LightingMaterials::new(&mut images, &mut materials, masks)
    });
    world.insert_resource(pool);
}

/// Registers material selection for headless consumers without shader setup.
pub fn add_classic_surface_system(app: &mut App) {
    app.add_systems(Update, apply_classic_surfaces.in_set(LightingSet::Apply));
}

#[derive(Component)]
struct MovingSurface;

fn apply_classic_surfaces(
    pool: Option<Res<LightingMaterials>>,
    grid: Option<Res<FineLightGrid>>,
    mut surfaces: Query<
        (&ClassicSurface, &mut SpriteMaterial<KonungLightMaterial>),
        With<MovingSurface>,
    >,
) {
    let (Some(pool), Some(grid)) = (pool, grid) else {
        return;
    };
    for (surface, mut material) in &mut surfaces {
        let ClassicSurface::Moving { cell: Some(cell) } = surface else {
            continue;
        };
        let handle = if grid.value(*cell) == 0 {
            &pool.ambient_plain
        } else {
            &pool.neutral
        };
        if material.0 != *handle {
            material.0 = handle.clone();
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// The WESL transcription stays in step with the CPU reference: the gate
    /// scales the sampled mask, the sign branches and the constants of
    /// `k1_game::presentation::lighting::apply_channel` are all present. A structural check,
    /// not a proof — the CPU reference and its own tests carry the formula.
    #[test]
    fn the_shader_transcribes_the_reference_formula() {
        let wesl = include_str!("shaders/terrain_light_material.wesl");
        // The window gate scales the sampled mask brightness.
        assert!(
            wesl.contains(
                "textureSample(mask_texture, mask_sampler, input.uv).r * terrain_light.light_gain"
            ),
            "the mask sample must be scaled by the window gate"
        );
        assert!(wesl.contains("light_gain: f32"), "the uniform struct field");
        // The per-channel constants of the reference.
        assert!(wesl.contains(">= 0.01"));
        assert!(wesl.contains("clamp(terrain_light.blend.rg + vec2<f32>(lightning)"));
        assert!(wesl.contains("linear_rgb_to_srgb(sprite_color.rgb)"));
        assert!(wesl.contains("srgb_to_linear_rgb(rgb)"));
        // Blue multiplies raw, without the light term.
        assert!(wesl.contains("rgb.b = rgb.b * (1.0 + terrain_light.blend.b)"));
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
    fn pool_serves_ambient_neutral_and_masked_materials() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let pool = LightingMaterials::new(
            &mut images,
            &mut materials,
            &[(vec![1u8; 4], 2, 2), (vec![2u8; 4], 2, 2)],
        );
        assert_eq!(pool.len(), 5, "two masks, two ambient variants and neutral");
        assert_ne!(pool.neutral(), pool.ambient());
        assert_ne!(pool.neutral(), pool.terrain(None));
        assert_ne!(pool.terrain(Some(0)), pool.terrain(Some(1)));
        assert_ne!(pool.terrain(None), pool.terrain(Some(0)));
        assert_ne!(pool.ambient(), pool.terrain(None));
        // An index past the pool falls back to the plain terrain material
        // instead of panicking: mask indices come from the data.
        assert_eq!(pool.terrain(Some(2)), pool.terrain(None));
    }

    #[test]
    fn the_full_pool_is_nineteen_masks_plus_three_plain_variants() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        // LIGHTS.RES ships exactly 19 tile masks; the pool is that plus the
        // two ambient materials and the neutral variant.
        let masks: Vec<(Vec<u8>, u32, u32)> = (0..19).map(|_| (vec![0u8], 1, 1)).collect();
        let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
        assert_eq!(pool.len(), 22, "19 masks + terrain + ambient + neutral");
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
                light_gain: 0.0,
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

    /// A pool that appears while the state is already standing gets lit by it
    /// without waiting for the state to move: a map switch mid-phase must not
    /// render unlit up to the next flicker block.
    #[test]
    fn a_new_pool_is_lit_by_the_standing_state() {
        let masks: Vec<(Vec<u8>, u32, u32)> = (0..2).map(|_| (vec![1u8; 4], 2, 2)).collect();
        let mut app = App::new();
        app.insert_resource(LightingState {
            terrain_blend: Vec3::new(-0.3, -0.6, -0.2),
            ambient_blend: Vec3::new(-0.3, -0.6, -0.2),
            light_color: Vec3::new(0.1, 0.05, 0.0),
            light_gain: 1.0,
        });
        app.insert_resource(Assets::<Image>::default());
        app.insert_resource(Assets::<KonungLightMaterial>::default());
        app.add_systems(Update, update_lighting_uniforms);
        // The state settles first, without any pool to receive it.
        app.update();

        // The pool arrives afterwards (a map switch): built against its own
        // fresh asset stores, then installed — the next update writes the
        // standing state into it.
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
        let handles: Vec<Handle<KonungLightMaterial>> = pool
            .masks
            .iter()
            .chain([&pool.terrain_plain, &pool.ambient_plain])
            .cloned()
            .collect();
        app.insert_resource(images);
        app.insert_resource(materials);
        app.insert_resource(pool);
        app.update();

        let materials = app.world().resource::<Assets<KonungLightMaterial>>();
        for (index, handle) in handles.iter().enumerate() {
            let material = materials.get(handle).expect("pool material alive");
            assert_eq!(
                material.blend,
                Vec3::new(-0.3, -0.6, -0.2),
                "material #{index} lit by the standing state"
            );
            assert_eq!(material.light_gain, 1.0, "material #{index} gate");
        }
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
            light_gain: 1.0,
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
                assert_eq!(material.light_gain, 1.0, "the gate reaches #{index}");
            }
        }

        // A new state — day, the ground window closed — rewrites them all: the
        // blends follow the phase, the gate closes for every material at once.
        app.insert_resource(LightingState {
            terrain_blend: Vec3::ZERO,
            ambient_blend: Vec3::new(0.1, 0.05, 0.0),
            light_color: Vec3::ZERO,
            light_gain: 0.0,
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
            assert_eq!(material.light_gain, 0.0, "the gate closes for #{index}");
        }
    }

    #[test]
    fn the_neutral_material_stays_unchanged_through_day_and_night() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let pool = LightingMaterials::new(&mut images, &mut materials, &[]);
        let neutral = pool.neutral();
        let mut app = App::new();
        app.insert_resource(images)
            .insert_resource(materials)
            .insert_resource(pool)
            .add_systems(Update, update_lighting_uniforms);
        for blend in [Vec3::splat(-0.7), Vec3::splat(0.2), Vec3::ZERO] {
            app.insert_resource(LightingState {
                terrain_blend: blend,
                ambient_blend: blend,
                light_color: Vec3::splat(0.1),
                light_gain: 1.0,
            });
            app.update();
            let material = app
                .world()
                .resource::<Assets<KonungLightMaterial>>()
                .get(&neutral)
                .unwrap();
            assert_eq!(material.blend, Vec3::ZERO);
            assert_eq!(material.light_color, Vec3::ZERO);
            assert_eq!(material.light_gain, 0.0);
            let mask = app
                .world()
                .resource::<Assets<Image>>()
                .get(&material.mask)
                .unwrap();
            assert_eq!(mask.data.as_deref(), Some([0].as_slice()));
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
