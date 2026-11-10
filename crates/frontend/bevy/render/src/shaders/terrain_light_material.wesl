// Ambient lighting for terrain sprites: the original's per-channel blend
// described in our own words from the reference fragment shader (Slavik
// gfx.cpp:56-84); the CPU reference it must match is `k1_game::lighting::
// apply_channel`. `blend` is the ambient /100 (negative at night), `light_color`
// is the tile-light colour /200, `light_gain` is the ground-light window gate
// (1.0 open, 0.0 closed — it switches the tile light only, never the ambient
// fade), and the mask texture carries the tile's LIGHTS.RES brightness in `.r`.
// The bindings start at 20 so they do not collide with the sprite's own
// material bindings (0-2).

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import bevy_sprite_render::sprite_mesh::functions as sprite_functions;

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
    var rgb = sprite_color.rgb;

    // A closed window leaves the mask inert: no light colour, no cancellation.
    let lightning = textureSample(mask_texture, mask_sampler, input.uv).r * terrain_light.light_gain;

    // Red and green: a negative blend darkens, the mask both cancels part of
    // the darkening and adds the tile light; a non-negative blend fades toward
    // white.
    if (terrain_light.blend.r < 0.0) {
        if (lightning >= 0.01) {
            rgb.r = rgb.r + (1.0 - rgb.r) * terrain_light.light_color.r;
        }
        rgb.r = rgb.r * (1.0 + clamp(terrain_light.blend.r + lightning, -1.0, 0.0));
    } else {
        rgb.r = rgb.r + (1.0 - rgb.r) * terrain_light.blend.r;
    }
    if (terrain_light.blend.g < 0.0) {
        if (lightning >= 0.01) {
            rgb.g = rgb.g + (1.0 - rgb.g) * terrain_light.light_color.g;
        }
        rgb.g = rgb.g * (1.0 + clamp(terrain_light.blend.g + lightning, -1.0, 0.0));
    } else {
        rgb.g = rgb.g + (1.0 - rgb.g) * terrain_light.blend.g;
    }
    // Blue never gets the tile light, and its negative multiply is unclamped
    // in the reference.
    if (terrain_light.blend.b < 0.0) {
        rgb.b = rgb.b * (1.0 + terrain_light.blend.b);
    } else {
        rgb.b = rgb.b + (1.0 - rgb.b) * terrain_light.blend.b;
    }

    return sprite_functions::get_final_color(vec4<f32>(rgb, sprite_color.a), input.instance_index);
}
