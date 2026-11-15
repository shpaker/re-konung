// K1 palette-space lighting; evidence and deliberate precision differences:
// docs/research/k1/lighting.md. `blend` is the signed ambient table factor,
// `light_color` the positive factor of integer-halved flicker levels, and
// `light_gain` gates only the tile mask. Bindings 0-2 belong to the sprite.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::sprite_mesh::functions as sprite_functions;

import bevy_render::color_operations::{linear_rgb_to_srgb, srgb_to_linear_rgb};

struct TerrainLight {
    blend: vec3<f32>,
    light_color: vec3<f32>,
    light_gain: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(20) var<uniform> terrain_light: TerrainLight;
@group(constants::MATERIAL_BIND_GROUP) @binding(21) var mask_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(22) var mask_sampler: sampler;

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let sprite_color = sprite_functions::sample_sprite_texture(input.uv, input.instance_index);
    // Palette arithmetic operates on display-encoded components. Bevy samples
    // sRGB textures into linear RGB; return to linear before tint and blending.
    var rgb = linear_rgb_to_srgb(sprite_color.rgb);

    // A closed window leaves the mask inert: no light colour, no cancellation.
    let lightning = textureSample(mask_texture, mask_sampler, input.uv).r * terrain_light.light_gain;

    // A nonzero mask selects the original's lit blit: brighten first, then
    // cancel negative ambient. Positive ambient is ignored on that path.
    if (lightning >= 0.01) {
        let lit = (rgb.rg + (vec2<f32>(1.0) - rgb.rg) * terrain_light.light_color.rg)
            * (vec2<f32>(1.0) + clamp(terrain_light.blend.rg + vec2<f32>(lightning), vec2<f32>(-1.0), vec2<f32>(0.0)));
        rgb.r = lit.r;
        rgb.g = lit.g;
    } else {
        rgb.r = select(rgb.r + (1.0 - rgb.r) * terrain_light.blend.r,
            rgb.r * (1.0 + terrain_light.blend.r), terrain_light.blend.r < 0.0);
        rgb.g = select(rgb.g + (1.0 - rgb.g) * terrain_light.blend.g,
            rgb.g * (1.0 + terrain_light.blend.g), terrain_light.blend.g < 0.0);
    }
    // Blue never gets the tile light, and its negative multiply is unclamped
    // in the reference.
    if (terrain_light.blend.b < 0.0) {
        rgb.b = rgb.b * (1.0 + terrain_light.blend.b);
    } else {
        rgb.b = rgb.b + (1.0 - rgb.b) * terrain_light.blend.b;
    }

    return sprite_functions::get_final_color(vec4<f32>(srgb_to_linear_rgb(rgb), sprite_color.a), input.instance_index);
}
