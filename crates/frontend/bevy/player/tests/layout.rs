//! Real text measurement and UI layout, with no GPU or native window server.
use bevy::prelude::*;
use bevy::ui::widget::Text;
use kn_app::{Action, MainMenuAction};
use kn_fe_player::flow::{AppFlow, PlayerFlowPlugin};
use kn_fe_player::widgets::character_sheet::{DetailText, StatAction};

fn app(width: u32, height: u32, scale: f32) -> App {
    let mut app = App::new();
    app.add_plugins(bevy::asset::AssetPlugin::default())
        .init_asset::<bevy::shader::Shader>();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<bevy::asset::AssetPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::render::RenderPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
            .disable::<bevy::core_pipeline::CorePipelinePlugin>()
            .disable::<bevy::post_process::PostProcessPlugin>()
            .disable::<bevy::sprite_render::SpriteRenderPlugin>()
            .disable::<bevy::ui_render::UiRenderPlugin>()
            .disable::<bevy::gizmos::GizmoPlugin>()
            .disable::<bevy::gizmos_render::GizmoRenderPlugin>()
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: (width, height).into(),
                    ..default()
                }),
                ..default()
            }),
    );
    app.add_plugins(kn_ui::KonungUiPlugin)
        .insert_resource(UiScale(scale))
        .init_resource::<kn_render::CreatureSim>()
        .init_resource::<kn_render::CreatureMotion>()
        .add_plugins(PlayerFlowPlugin {
            heroes: (0..8).collect(),
            settings: kn_settings::defaults(),
            settings_path: None,
        });
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: bevy::camera::ComputedCameraValues {
                target_info: Some(bevy::camera::RenderTargetInfo {
                    physical_size: UVec2::new(width, height),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            viewport: Some(bevy::camera::Viewport {
                physical_size: UVec2::new(width, height),
                ..default()
            }),
            ..default()
        },
    ));
    app.finish();
    app.cleanup();
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    for _ in 0..8 {
        app.update();
    }
    app
}

fn bounds(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    let center = world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .affine()
        .translation;
    Rect::from_center_size(center, node.size)
}

#[test]
fn creation_layout_keeps_actions_inside_the_window_at_different_sizes_and_scales() {
    for (width, height, scale) in [
        (1280, 800, 1.0),
        (1024, 768, 1.0),
        (900, 700, 1.0),
        (640, 800, 1.0),
        (1280, 800, 1.5),
    ] {
        let mut app = app(width, height, scale);
        let begin_text = app
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .find(|(_, text)| text.0 == "Начать")
            .unwrap()
            .0;
        let begin = bounds(app.world(), begin_text);
        assert!(
            begin.min.x >= 0.0 && begin.max.x <= width as f32 && begin.max.y <= height as f32,
            "footer clipped at {width}x{height} scale {scale}: {begin:?}"
        );
        let actions = app
            .world_mut()
            .query::<(Entity, &StatAction)>()
            .iter(app.world())
            .map(|(entity, action)| (entity, *action))
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 8);
        for (entity, _) in &actions {
            let area = bounds(app.world(), *entity);
            assert!(
                area.width() > 0.0 && area.min.x >= 0.0 && area.max.x <= width as f32,
                "attribute action clipped at {width} scale {scale}: {area:?}"
            );
            if width as f32 / scale >= 960.0 {
                assert!(
                    area.min.y >= 0.0 && area.max.y < begin.min.y,
                    "all four attributes must be visible: {area:?}"
                );
            }
        }
        let detail = app
            .world_mut()
            .query_filtered::<Entity, With<DetailText>>()
            .single(app.world())
            .unwrap();
        let attribute = bounds(app.world(), actions[0].0);
        let detail = bounds(app.world(), detail);
        if width as f32 / scale < 960.0 {
            assert!(
                attribute.max.y < detail.min.y,
                "narrow sheet must stack its columns"
            );
        } else {
            assert!(
                attribute.max.x < detail.min.x,
                "wide sheet must keep its columns side by side: {width} {scale} attribute={attribute:?} detail={detail:?}"
            );
        }
        let last_hero = app
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .find(|(_, text)| text.0 == "Герой 8")
            .unwrap()
            .0;
        let mut card = last_hero;
        while app.world().get::<bevy::ui_widgets::Button>(card).is_none() {
            card = app.world().get::<ChildOf>(card).unwrap().parent();
        }
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(card, bevy::input_focus::FocusCause::Navigated);
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocusVisible>()
            .0 = true;
        for _ in 0..3 {
            app.update();
        }
        let last = bounds(app.world(), card);
        assert!(
            last.min.y >= 0.0
                && last.max.y < begin.min.y
                && last.min.x >= 0.0
                && last.max.x <= width as f32,
            "keyboard navigation must reveal the eighth hero: {last:?}"
        );
    }
}

#[test]
fn shared_sheet_lays_out_long_synthetic_stats_and_read_only_values() {
    use kn_fe_player::widgets::character_sheet::{
        self, CharacterSheet, SheetModel, Stat, StatEdit, StatGroup, StatOperation,
    };
    for (count, editable) in [(6, true), (10, true), (10, false)] {
        let mut app = app(1024, 768, 1.0);
        let old = app
            .world_mut()
            .query_filtered::<Entity, With<SheetModel>>()
            .single(app.world())
            .unwrap();
        let parent = app.world().get::<ChildOf>(old).unwrap().parent();
        app.world_mut().despawn(old);
        let fonts = app.world().resource::<kn_ui::KonungFonts>().clone();
        let data = CharacterSheet {
            character: 42,
            name: "Synthetic character".into(),
            history: "Synthetic biography. ".repeat(180),
            groups: vec![StatGroup {
                placement: character_sheet::GroupPlacement::Statistics,
                label: "Attributes".into(),
                stats: (0..count)
                    .map(|id| Stat {
                        id,
                        label: format!("Очень длинное название характеристики номер {id}"),
                        value: (125 + id).to_string(),
                        description: "Synthetic description. ".repeat(100),
                        edit: editable.then(|| StatEdit {
                            baseline: "100".into(),
                            increase: StatOperation {
                                summary: "Increase: 2 points".into(),
                                blocked: None,
                            },
                            decrease: StatOperation {
                                summary: "Decrease: +2 points".into(),
                                blocked: None,
                            },
                        }),
                    })
                    .collect(),
            }],
            ..default()
        };
        app.world_mut()
            .spawn_scene(character_sheet::sheet(&fonts, data, bsn_list! {}))
            .unwrap()
            .insert(ChildOf(parent));
        for _ in 0..4 {
            app.update();
        }
        let rows = app
            .world_mut()
            .query::<(Entity, &character_sheet::DetailSource)>()
            .iter(app.world())
            .filter(|(entity, source)| {
                source.0 < 1000 && app.world().get::<StatAction>(*entity).is_none()
            })
            .map(|(entity, source)| (entity, source.0))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), count);
        let mut areas = rows
            .iter()
            .map(|(entity, _)| bounds(app.world(), *entity))
            .collect::<Vec<_>>();
        areas.sort_by(|left, right| left.min.y.total_cmp(&right.min.y));
        for pair in areas.windows(2) {
            assert!(pair[0].max.y <= pair[1].min.y, "stat rows overlap");
        }
        let actions = app
            .world_mut()
            .query::<(Entity, &StatAction)>()
            .iter(app.world())
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), if editable { count * 2 } else { 0 });
        for entity in actions {
            assert!(bounds(app.world(), entity).max.x <= 1024.0);
        }
        let (_, last) = rows.iter().max_by_key(|(_, id)| id).unwrap();
        app.world_mut()
            .resource_mut::<character_sheet::SheetInteraction>()
            .selected = Some(*last);
        app.update();
        let detail = &app
            .world_mut()
            .query_filtered::<&Text, With<DetailText>>()
            .single(app.world())
            .unwrap()
            .0;
        assert!(
            detail.contains(&format!("номер {last}")),
            "the full clipped name remains available in the context panel"
        );
        assert_eq!(detail.contains("Increase:"), editable);
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .single(app.world())
            .unwrap();
        let regions = app
            .world_mut()
            .query::<(Entity, &character_sheet::SheetScroll, &ScrollPosition)>()
            .iter(app.world())
            .map(|(entity, id, position)| (id.0, entity, position.0))
            .collect::<Vec<_>>();
        let history = regions.iter().find(|(id, _, _)| *id == 4).unwrap().1;
        let context = regions.iter().find(|(id, _, _)| *id == 3).unwrap().1;
        for target in [history, context] {
            app.world_mut()
                .trigger(bevy::picking::events::PointerScroll {
                    entity: target,
                    pointer: bevy::picking::events::Pointer::new(
                        bevy::picking::pointer::PointerId::Mouse,
                        bevy::picking::pointer::Location {
                            target: bevy::camera::NormalizedRenderTarget::Window(
                                bevy::window::WindowRef::Entity(window)
                                    .normalize(Some(window))
                                    .unwrap(),
                            ),
                            position: Vec2::ZERO,
                        },
                    ),
                    unit: bevy::input::mouse::MouseScrollUnit::Line,
                    x: 0.0,
                    y: -5.0,
                    hit: bevy::picking::backend::HitData::new(window, 0.0, None, None),
                    phase: bevy::input::touch::TouchPhase::Moved,
                });
            app.update();
            assert!(app.world().get::<ScrollPosition>(target).unwrap().y > 0.0);
        }
        app.world_mut()
            .resource_mut::<character_sheet::SheetInteraction>()
            .selected = Some(0);
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(context).unwrap().y,
            0.0,
            "new details must reveal their heading and prices"
        );
        assert!(app.world().get::<ScrollPosition>(history).unwrap().y > 0.0);
        for (id, entity, position) in regions {
            if ![3, 4].contains(&id) {
                assert_eq!(
                    app.world().get::<ScrollPosition>(entity).unwrap().0,
                    position,
                    "scroll must stay in its own region"
                );
            }
        }
    }
}
