//! Ambient lighting of terrain pieces: the sprite material extension with the
//! light mask, and the pool of materials per mask.
//!
//! The fragment shader reproduces the original's per-channel blend (Slavik
//! `gfx.cpp:56-84`); the CPU reference it must agree with is
//! `k1_game::lighting::apply_channel`. The render layer stays K1-free: the
//! caller hands over ready-made uniform values and raw mask brightness bytes,
//! exactly the numbers the original passed to its shader.
//!
//! Spike: the pool is filled by the viewer from the CLI flags; the lighting
//! plugin with the time contract will own both later.

use bevy::{
    asset::{AssetPath, RenderAssetUsages},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{MaterialExtension2d, SpriteMaterialPlugin},
};

/// The material's fragment shader, embedded next to this module. The path is
/// spelled out twice because `load_shader_library!` only takes a literal.
const SHADER_PATH: &str = "shaders/terrain_light_material.wesl";

/// Per-terrain-sprite lighting: the ambient blend and the tile-light colour as
/// uniforms, the tile's LIGHTS.RES mask as a second texture.
///
/// The uniforms start at 20 so they do not collide with the sprite's own
/// material bindings (0-2). A mask brightness byte `v` becomes
/// `round(v / 100 * 255)`, so the shader's sampled `.r` is the original's
/// `lightning` (`v / 100`).
#[derive(AsBindGroup, Asset, Reflect, Clone)]
pub struct TerrainLightMaterial {
    /// Ambient /100: negative darkens, positive fades toward white.
    #[uniform(20)]
    blend: Vec3,
    /// Tile-light colour /200, added toward white where the mask is bright.
    #[uniform(20)]
    light_color: Vec3,
    /// LIGHTS.RES tile mask; brightness in `.r`. A piece without a mask gets a
    /// 1×1 zero texture, so `lightning` stays 0 and the formula is uniform for
    /// every terrain sprite.
    #[texture(21)]
    #[sampler(22)]
    mask: Handle<Image>,
}

impl MaterialExtension2d for TerrainLightMaterial {
    fn fragment_shader() -> Option<ShaderRef> {
        Some(ShaderRef::Path(
            AssetPath::from_path_buf(bevy::asset::embedded_path!(SHADER_PATH))
                .with_source("embedded"),
        ))
    }
}

/// Registers the terrain-light material and its embedded shader.
///
/// The shader is embedded and loaded permanently (not on demand) so the WESL
/// imports from `bevy_sprite_render` resolve: the import resolver sees only
/// shader assets that are loaded.
pub struct TerrainLightMaterialPlugin;

impl Plugin for TerrainLightMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::shader::load_shader_library!(app, "shaders/terrain_light_material.wesl");
        app.add_plugins(SpriteMaterialPlugin::<TerrainLightMaterial>::default());
    }
}

/// Pool of terrain-light materials: one per mask, plus one for pieces without
/// a mask. Handles are shared by every sprite with the same mask, so a map
/// yields at most 20 materials however many tiles it has.
#[derive(Resource)]
pub struct TerrainLightMaterials {
    no_mask: Handle<TerrainLightMaterial>,
    masks: Vec<Handle<TerrainLightMaterial>>,
}

impl TerrainLightMaterials {
    /// Builds the pool from raw mask brightness bytes; one handle per mask,
    /// indexed by `light_mask_index` (0-based). `width`/`height` are the tile
    /// mask dimensions the bytes are laid out for.
    pub fn new(
        images: &mut Assets<Image>,
        materials: &mut Assets<TerrainLightMaterial>,
        blend: Vec3,
        light_color: Vec3,
        masks: &[(Vec<u8>, u32, u32)],
    ) -> Self {
        let mut material = |mask: Handle<Image>| {
            materials.add(TerrainLightMaterial {
                blend,
                light_color,
                mask,
            })
        };
        let no_mask = material(images.add(zero_mask_image()));
        let masks = masks
            .iter()
            .map(|(mask, width, height)| {
                material(images.add(light_mask_image(mask, *width, *height)))
            })
            .collect();
        Self { no_mask, masks }
    }

    /// The material for a piece with this mask index; `None` (or an index past
    /// the pool) falls back to the no-mask material.
    pub fn handle(&self, mask_index: Option<usize>) -> Handle<TerrainLightMaterial> {
        mask_index
            .and_then(|index| self.masks.get(index))
            .unwrap_or(&self.no_mask)
            .clone()
    }

    /// How many materials the pool holds: the masks plus the no-mask one.
    pub fn len(&self) -> usize {
        self.masks.len() + 1
    }

    pub fn is_empty(&self) -> bool {
        false
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
    fn pool_serves_one_material_per_mask_and_falls_back() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<TerrainLightMaterial>::default();
        let pool = TerrainLightMaterials::new(
            &mut images,
            &mut materials,
            Vec3::NEG_ONE,
            Vec3::ZERO,
            &[(vec![1u8; 4], 2, 2), (vec![2u8; 4], 2, 2)],
        );
        assert_eq!(pool.len(), 3);
        assert_ne!(pool.handle(Some(0)), pool.handle(Some(1)));
        assert_ne!(pool.handle(None), pool.handle(Some(0)));
        // An index past the pool falls back to the no-mask material instead of
        // panicking: mask indices come from the data.
        assert_eq!(pool.handle(Some(2)), pool.handle(None));
    }
}
