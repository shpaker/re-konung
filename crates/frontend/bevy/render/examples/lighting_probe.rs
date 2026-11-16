//! Synthetic GPU regression probe; run with an output PNG path, then validate
//! its center pixels with `tools/check_lighting_probe.py`. No game data is used.
use bevy::{
    asset::RenderAssetUsages,
    ecs::system::RunSystemOnce,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Capturing, Screenshot, save_to_disk},
    },
    sprite_render::SpriteMaterial,
    window::WindowResolution,
};
use kn_render::{
    KonungLightMaterial, KonungLightingPlugin, LightingMaterials, LightingState,
    update_lighting_uniforms,
};

#[derive(Component)]
struct BoundaryProbe;

#[derive(Resource)]
struct BoundaryMaterials {
    ambient: Handle<KonungLightMaterial>,
    neutral: Handle<KonungLightMaterial>,
    switches: u32,
}

fn switch_material(
    mut materials: ResMut<BoundaryMaterials>,
    mut sprite: Single<&mut SpriteMaterial<KonungLightMaterial>, With<BoundaryProbe>>,
) {
    if materials.switches < 240 {
        sprite.0 = if materials.switches.is_multiple_of(2) {
            materials.ambient.clone()
        } else {
            materials.neutral.clone()
        };
        materials.switches += 1;
    }
}

fn setup(world: &mut World) {
    world.spawn(Camera2d);
    let texture = world.resource_mut::<Assets<Image>>().add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![128, 128, 128, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let cases = [
        ([0.0; 3], 0),
        ([-0.6, -0.7, -0.7], 0),
        ([0.12, 0.06, 0.0], 0),
        ([-0.6, -0.7, -0.7], 20),
        ([0.1, -0.2, -0.5], 20),
        ([0.0; 3], 20),
    ];
    for (index, (ambient, mask)) in cases.into_iter().enumerate() {
        let pool = world.resource_scope::<Assets<Image>, _>(|world, mut images| {
            let mut materials = world.resource_mut::<Assets<KonungLightMaterial>>();
            LightingMaterials::new(&mut images, &mut materials, &[(vec![mask], 1, 1)])
        });
        let material = pool.terrain(Some(0));
        world.insert_resource(pool);
        world.insert_resource(LightingState {
            terrain_blend: Vec3::from(ambient),
            ambient_blend: Vec3::from(ambient),
            light_color: Vec3::new(0.08, 0.04, 0.0),
            light_gain: 1.0,
        });
        world.run_system_once(update_lighting_uniforms).unwrap();
        world.spawn((
            Sprite {
                image: texture.clone(),
                custom_size: Some(Vec2::splat(64.0)),
                ..default()
            },
            SpriteMaterial(material),
            Transform::from_xyz(index as f32 * 64.0 - 160.0, 0.0, 0.0),
        ));
    }
    let pool = world.resource_scope::<Assets<Image>, _>(|world, mut images| {
        let mut materials = world.resource_mut::<Assets<KonungLightMaterial>>();
        LightingMaterials::new(&mut images, &mut materials, &[])
    });
    let ambient = pool.ambient();
    let neutral = pool.neutral();
    world.insert_resource(pool);
    world.insert_resource(LightingState {
        ambient_blend: Vec3::splat(-0.7),
        ..default()
    });
    world.run_system_once(update_lighting_uniforms).unwrap();
    world.insert_resource(BoundaryMaterials {
        ambient,
        neutral: neutral.clone(),
        switches: 0,
    });
    for (x, extended) in [(-64.0, true), (64.0, false)] {
        let mut sprite = world.spawn((
            Sprite {
                image: texture.clone(),
                color: Color::srgba(0.6, 0.8, 1.0, 0.5),
                custom_size: Some(Vec2::splat(64.0)),
                ..default()
            },
            Transform::from_xyz(x, -80.0, 0.0),
        ));
        if extended {
            sprite.insert((BoundaryProbe, SpriteMaterial(neutral.clone())));
        }
    }
    // The probe freezes every material at its own case instead of using a shared clock.
    world.remove_resource::<LightingMaterials>();
}

fn capture(
    mut commands: Commands,
    mut frames: Local<u32>,
    capturing: Query<(), With<Capturing>>,
    time: Res<Time>,
    boundary: Res<BoundaryMaterials>,
) {
    if time.elapsed_secs() < 3.0 || boundary.switches < 240 {
        return;
    }
    *frames += 1;
    if *frames == 40 {
        let path = std::env::args().nth(1).expect("output PNG path");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    } else if *frames > 45 && capturing.is_empty() {
        commands.write_message(AppExit::Success);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Lighting pixel probe".into(),
                resolution: WindowResolution::new(384, 256).with_scale_factor_override(1.0),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(KonungLightingPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (switch_material, capture).chain())
        .run();
}
