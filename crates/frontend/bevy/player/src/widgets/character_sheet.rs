//! Data-driven character inspection and allocation. Rules and localized game
//! descriptions arrive from the caller; this view never computes game values.

use bevy::feathers::font_styles::InheritableFont;
use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible, tab_navigation::TabIndex};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene, SceneList};
use bevy::text::FontWeight;
use bevy::ui::widget::{ImageNode, Text};
use bevy::ui_widgets::Activate;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

/// One active character sheet, shared by creation and in-game inspection.
pub struct CharacterSheetPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SheetSync;

impl Plugin for CharacterSheetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SheetInteraction>()
            .add_systems(Update, sync.in_set(SheetSync))
            .add_systems(
                PostUpdate,
                reveal_focus.after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

#[derive(Clone)]
pub struct SheetImage {
    pub handle: Handle<Image>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone)]
pub struct StatOperation {
    pub summary: String,
    pub blocked: Option<String>,
}

#[derive(Clone)]
pub struct StatEdit {
    pub baseline: String,
    pub decrease: StatOperation,
    pub increase: StatOperation,
}

#[derive(Clone)]
pub struct Stat {
    pub id: usize,
    pub label: String,
    pub value: String,
    pub description: String,
    pub edit: Option<StatEdit>,
}

/// Presentation slots, independent of a game's stat names or number of rows.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum GroupPlacement {
    BesideImage,
    BelowHistory,
    #[default]
    Statistics,
}

#[derive(Clone)]
pub struct StatGroup {
    pub placement: GroupPlacement,
    pub label: String,
    pub stats: Vec<Stat>,
}

#[derive(Clone, Default)]
pub struct CharacterSheet {
    pub character: u32,
    pub name: String,
    pub image: Option<SheetImage>,
    pub history: String,
    pub baseline_label: String,
    pub guidance: String,
    pub groups: Vec<StatGroup>,
    pub extra_details: Vec<Stat>,
}

#[derive(Component, Clone, Default)]
pub struct SheetModel(pub std::sync::Arc<CharacterSheet>);

impl CharacterSheet {
    fn stat(&self, id: usize) -> Option<&Stat> {
        self.groups
            .iter()
            .flat_map(|group| &group.stats)
            .chain(&self.extra_details)
            .find(|stat| stat.id == id)
    }

    fn detail(&self, id: usize) -> String {
        let Some(stat) = self.stat(id) else {
            return self.guidance.clone();
        };
        let mut parts = Vec::new();
        if !stat.label.is_empty() {
            parts.push(stat.label.clone());
        }
        if let Some(edit) = &stat.edit {
            for operation in [&edit.increase, &edit.decrease] {
                if !operation.summary.is_empty() {
                    parts.push(operation.summary.clone());
                }
                if let Some(reason) = &operation.blocked
                    && !parts.contains(reason)
                {
                    parts.push(reason.clone());
                }
            }
            parts.push(format!("{}: {}", self.baseline_label, edit.baseline));
        }
        if !stat.description.is_empty() {
            parts.push(if parts.is_empty() {
                stat.description.clone()
            } else {
                format!("\n{}", stat.description)
            });
        }
        parts.join("\n")
    }
}

#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetailSource(pub usize);

#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatAction {
    pub attribute: usize,
    pub delta: i16,
}

#[derive(Event)]
pub struct AdjustStat(pub StatAction);

#[derive(Component, Clone, Default)]
pub struct DetailText;
#[derive(Component, Clone, Default)]
pub struct HistoryText;
#[derive(Component, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SheetScroll(pub u8);
#[derive(Component, Clone, Default)]
struct StatRow;
#[derive(Component, Clone, Default)]
struct SheetColumns;
#[derive(Component, Clone, Default)]
struct IdentityColumn;
#[derive(Component, Clone, Default)]
struct StatisticsColumn;
#[derive(Component, Clone, Default)]
struct ContextColumn;

#[derive(Resource, Clone, Default)]
pub struct SheetInteraction {
    pub character: Option<u32>,
    pub selected: Option<usize>,
    last_focus: Option<Entity>,
    last_cursor: Option<Vec2>,
    last_hover: Option<usize>,
    shown_detail: Option<usize>,
    revealed_focus: Option<Entity>,
}

impl SheetInteraction {
    pub fn select_character(&mut self, character: Option<u32>) {
        if self.character != character {
            self.character = character;
        }
    }
}

fn groups(
    fonts: &KonungFonts,
    data: &CharacterSheet,
    placement: GroupPlacement,
) -> Vec<impl Scene> {
    data.groups.iter().filter(|group| group.placement == placement).map(|group| {
        let rows = group.stats.iter().map(|stat| stat_row(fonts, stat)).collect::<Vec<_>>();
        bsn! {
            Node { flex_direction: FlexDirection::Column, min_width: px(0), width: percent(100), flex_shrink: 0.0 }
            Children [
                @label(fonts, group.label.as_str()) TextColor(palette::TEXT)
                Node { margin: UiRect::bottom(px(4)) }
                --
                {rows}
            ]
        }
    }).collect()
}

pub fn sheet(fonts: &KonungFonts, data: CharacterSheet, settings: impl SceneList) -> impl Scene {
    let data = std::sync::Arc::new(data);
    let facts = groups(fonts, &data, GroupPlacement::BesideImage);
    let skills = groups(fonts, &data, GroupPlacement::BelowHistory);
    let statistics = groups(fonts, &data, GroupPlacement::Statistics);
    bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(16), row_gap: px(16), width: percent(100), height: percent(100), min_height: px(0), min_width: px(0), align_items: AlignItems::Start, overflow: Overflow::scroll_y() }
        SheetColumns SheetScroll(0) ScrollPosition::default() on(scroll)
        InheritableFont { font: {fonts.regular.clone()}, font_size: typo::BODY, weight: FontWeight::NORMAL }
        Children [
            @widgets::panel(fonts, bsn_list! {
                @label(fonts, data.name.as_str()) TextColor(palette::TEXT)
                --
                Node { width: percent(100), column_gap: px(8), min_height: px(120), flex_shrink: 0.0, align_items: AlignItems::Center }
                Children [
                    {EntityScene(image_view(data.image.clone()))}
                    --
                    Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), flex_direction: FlexDirection::Column }
                    Children [{facts}]
                ]
                --
                Node { height: px(156), min_height: px(156), width: percent(100), overflow: Overflow::scroll_y(), flex_shrink: 0.0 }
                SheetScroll(4) ScrollPosition::default() on(scroll)
                Children [
                    @label(fonts, data.history.as_str()) HistoryText TextColor(palette::TEXT)
                    Node { width: percent(100), flex_shrink: 0.0 }
                ]
                --
                {skills}
            })
            IdentityColumn SheetScroll(1) ScrollPosition::default() on(scroll)
            Node { width: px(320), height: percent(100), min_height: px(0), min_width: px(0), flex_shrink: 0.0, padding: UiRect::all(px(12)), row_gap: px(8), overflow: Overflow::scroll_y() }
            --
            @widgets::panel(fonts, bsn_list! {
                {statistics}
                --
                {settings}
            })
            StatisticsColumn SheetScroll(2) ScrollPosition::default() on(scroll)
            Node { width: px(384), height: percent(100), min_height: px(0), min_width: px(0), flex_shrink: 0.0, padding: UiRect::all(px(12)), row_gap: px(8), overflow: Overflow::scroll_y() }
            --
            @widgets::panel(fonts, bsn_list! {
                @label(fonts, data.guidance.as_str()) DetailText TextColor(palette::TEXT)
                Node { width: percent(100), flex_shrink: 0.0 }
            })
            ContextColumn SheetScroll(3) ScrollPosition::default() on(scroll)
            Node { width: px(304), height: percent(100), min_height: px(0), min_width: px(0), flex_shrink: 0.0, padding: UiRect::all(px(12)), overflow: Overflow::scroll_y() }
        ]
        SheetModel({data.clone()})
    }
}

pub fn image_view(image: Option<SheetImage>) -> Box<dyn Scene> {
    match image {
        Some(image) => Box::new(bsn! {
            Node { width: px(image.width), height: px(image.height), flex_shrink: 0.0 }
            ImageNode { image: {image.handle} }
            bevy::picking::Pickable::IGNORE
        }),
        None => Box::new(bsn! { Node { width: px(76), height: px(87) } }),
    }
}

fn label(fonts: &KonungFonts, text: &str) -> impl Scene {
    bsn! {
        Text(text)
        TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: typo::BODY }
    }
}

fn stat_row(fonts: &KonungFonts, stat: &Stat) -> impl Scene {
    let changed = stat
        .edit
        .as_ref()
        .is_some_and(|edit| edit.baseline != stat.value);
    let minus = stat
        .edit
        .as_ref()
        .map(|edit| operation(fonts, stat.id, -1, &edit.decrease))
        .into_iter()
        .collect::<Vec<_>>();
    let plus = stat
        .edit
        .as_ref()
        .map(|edit| operation(fonts, stat.id, 1, &edit.increase))
        .into_iter()
        .collect::<Vec<_>>();
    let row_label = if changed {
        format!("{} ·", stat.label)
    } else {
        stat.label.clone()
    };
    let height = if stat.edit.is_some() { px(36) } else { px(24) };
    let color = if changed {
        palette::ACCENT
    } else {
        palette::TEXT
    };
    bsn! {
        Node { height: height, flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: spacing::SPACE_4, width: percent(100) }
        DetailSource({stat.id}) StatRow Hovered TabIndex(0) on(select_detail)
        BackgroundColor(Color::NONE)
        Children [
            @label(fonts, row_label.as_str()) TextColor(palette::TEXT_MUTED)
            Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), overflow: Overflow::clip() }
            TextLayout { linebreak: bevy::text::LineBreak::NoWrap }
            --
            {minus}
            --
            @label(fonts, stat.value.as_str()) TextColor(color)
            Node { min_width: px(32), justify_content: JustifyContent::Center }
            --
            {plus}
        ]
    }
}

fn operation(
    fonts: &KonungFonts,
    id: usize,
    delta: i16,
    operation: &StatOperation,
) -> Box<dyn Scene> {
    if operation.blocked.is_some() {
        Box::new(
            bsn! { @operation_button(fonts, id, delta) bevy::ui::InteractionDisabled BorderColor::all(palette::BORDER) },
        )
    } else {
        Box::new(operation_button(fonts, id, delta))
    }
}

fn operation_button(fonts: &KonungFonts, id: usize, delta: i16) -> impl Scene {
    let label = if delta < 0 { "−" } else { "+" };
    bsn! {
        @widgets::menu_button(fonts, label)
        Node { width: px(32), height: px(32), padding: UiRect::ZERO }
        StatAction { attribute: id, delta } DetailSource(id) on(adjust)
    }
}

fn adjust(
    event: On<Activate>,
    actions: Query<&StatAction, Without<bevy::ui::InteractionDisabled>>,
    mut commands: Commands,
    mut state: ResMut<SheetInteraction>,
) {
    if let Ok(action) = actions.get(event.entity) {
        state.selected = Some(action.attribute);
        commands.trigger(AdjustStat(*action));
    }
}

fn select_detail(
    event: On<bevy::picking::events::PointerClick>,
    sources: Query<&DetailSource>,
    mut state: ResMut<SheetInteraction>,
    mut focus: ResMut<InputFocus>,
) {
    if let Ok(source) = sources.get(event.entity) {
        state.selected = Some(source.0);
        focus.set(event.entity, FocusCause::Pressed);
    }
}

pub fn scroll(
    mut event: On<bevy::picking::events::PointerScroll>,
    mut nodes: Query<(&mut ScrollPosition, &ComputedNode, &Node)>,
) {
    if let Ok((mut position, node, style)) = nodes.get_mut(event.entity) {
        let factor = match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => 32.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
        };
        let maximum = (node.content_size.y - node.size.y).max(0.0) * node.inverse_scale_factor;
        if style.overflow.x == OverflowAxis::Scroll && style.overflow.y != OverflowAxis::Scroll {
            let maximum = (node.content_size.x - node.size.x).max(0.0) * node.inverse_scale_factor;
            position.x = (position.x - (event.x + event.y) * factor).clamp(0.0, maximum);
        } else {
            position.y = (position.y - event.y * factor).clamp(0.0, maximum);
        }
        event.propagate(false);
    }
}

pub fn sync(world: &mut World) {
    let Some(data) = world
        .query::<&SheetModel>()
        .iter(world)
        .next()
        .map(|model| model.0.clone())
    else {
        *world.resource_mut::<SheetInteraction>() = SheetInteraction::default();
        return;
    };
    let (cursor, width) = world
        .query::<&Window>()
        .iter(world)
        .next()
        .map_or((None, 1280.0), |window| {
            (window.cursor_position(), window.width())
        });
    let focus = world
        .get_resource::<InputFocusVisible>()
        .filter(|visible| visible.0)
        .and_then(|_| world.get_resource::<InputFocus>())
        .and_then(InputFocus::get);
    let focused = focus
        .and_then(|entity| world.get::<DetailSource>(entity))
        .map(|source| source.0);
    let hovered = world
        .query::<(&DetailSource, &Hovered)>()
        .iter(world)
        .find(|(_, hovered)| hovered.0)
        .map(|(source, _)| source.0);
    let mut state = world.resource::<SheetInteraction>().clone();
    let pointer_changed = cursor != state.last_cursor || hovered != state.last_hover;
    if focus != state.last_focus && focused.is_some() {
        state.selected = focused;
    } else if pointer_changed && hovered.is_some() {
        state.selected = hovered;
    }
    if state.selected.is_none_or(|id| data.stat(id).is_none()) {
        state.selected = data
            .groups
            .iter()
            .flat_map(|group| &group.stats)
            .next()
            .map(|stat| stat.id);
    }
    if state.shown_detail != state.selected {
        for (region, mut position) in world
            .query::<(&SheetScroll, &mut ScrollPosition)>()
            .iter_mut(world)
        {
            if region.0 == 3 {
                position.0 = Vec2::ZERO;
            }
        }
        state.shown_detail = state.selected;
    }
    state.last_focus = focus;
    state.last_cursor = cursor;
    state.last_hover = hovered;
    let text = state
        .selected
        .map_or_else(|| data.guidance.clone(), |id| data.detail(id));
    for mut detail in world
        .query_filtered::<&mut Text, With<DetailText>>()
        .iter_mut(world)
    {
        if detail.0 != text {
            detail.0.clone_from(&text);
        }
    }
    for (source, mut background) in world
        .query_filtered::<(&DetailSource, &mut BackgroundColor), With<StatRow>>()
        .iter_mut(world)
    {
        background.0 = if focused == Some(source.0) {
            palette::SURFACE_RAISED
        } else {
            Color::NONE
        };
    }
    let scale = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
    let logical_width = width / scale;
    let available = (logical_width - 48.0).clamp(1.0, 1040.0);
    let stacked = logical_width < 720.0;
    let narrow = logical_width < 960.0;
    for mut node in world
        .query_filtered::<&mut Node, With<SheetColumns>>()
        .iter_mut(world)
    {
        node.flex_wrap = if narrow {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        };
    }
    let content_width = available
        - if stacked {
            0.0
        } else if narrow {
            16.0
        } else {
            32.0
        };
    for (mut node, identity, statistics, context) in world
        .query::<(
            &mut Node,
            Has<IdentityColumn>,
            Has<StatisticsColumn>,
            Has<ContextColumn>,
        )>()
        .iter_mut(world)
    {
        if !identity && !statistics && !context {
            continue;
        }
        node.width = if stacked || (narrow && context) {
            percent(100)
        } else if narrow {
            px((content_width
                * if identity {
                    320.0 / 704.0
                } else {
                    384.0 / 704.0
                })
            .floor())
        } else {
            px(content_width
                * if identity {
                    320.0 / 1008.0
                } else if statistics {
                    384.0 / 1008.0
                } else {
                    304.0 / 1008.0
                })
        };
        node.height = if narrow {
            px(if context { 220.0 } else { 440.0 })
        } else {
            percent(100)
        };
    }
    world.insert_resource(state);
}

/// Keep keyboard navigation visible inside independently scrolling columns.
pub fn reveal_focus(world: &mut World) {
    if !world
        .get_resource::<InputFocusVisible>()
        .is_some_and(|visible| visible.0)
    {
        return;
    }
    let Some(focus) = world.get_resource::<InputFocus>().and_then(InputFocus::get) else {
        return;
    };
    if world.resource::<SheetInteraction>().revealed_focus == Some(focus) {
        return;
    }
    let Some(target_node) = world.get::<ComputedNode>(focus) else {
        return;
    };
    if target_node.size.y == 0.0 {
        return;
    }
    let Some(target_transform) = world.get::<UiGlobalTransform>(focus) else {
        return;
    };
    let center = target_transform.affine().translation;
    let start = center - target_node.size / 2.0;
    let end = center + target_node.size / 2.0;
    let mut child = focus;
    while let Some(parent) = world.get::<ChildOf>(child).map(ChildOf::parent) {
        child = parent;
        if world.get::<ScrollPosition>(parent).is_none() {
            continue;
        }
        let Some(node) = world.get::<ComputedNode>(parent) else {
            continue;
        };
        let Some(transform) = world.get::<UiGlobalTransform>(parent) else {
            continue;
        };
        let viewport_center = transform.affine().translation;
        let viewport_start = viewport_center - node.size / 2.0;
        let viewport_end = viewport_center + node.size / 2.0;
        let delta = Vec2::new(
            if start.x < viewport_start.x {
                start.x - viewport_start.x
            } else if end.x > viewport_end.x {
                end.x - viewport_end.x
            } else {
                0.0
            },
            if start.y < viewport_start.y {
                start.y - viewport_start.y
            } else if end.y > viewport_end.y {
                end.y - viewport_end.y
            } else {
                0.0
            },
        );
        let maximum = (node.content_size - node.size).max(Vec2::ZERO) * node.inverse_scale_factor;
        let scale = node.inverse_scale_factor;
        if let Some(mut position) = world.get_mut::<ScrollPosition>(parent) {
            position.0 = (position.0 + delta * scale).clamp(Vec2::ZERO, maximum);
        }
    }
    world.resource_mut::<SheetInteraction>().revealed_focus = Some(focus);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (App, KonungFonts) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ))
        .init_asset::<Font>()
        .insert_resource(kn_ui::konung_theme())
        .init_resource::<SheetInteraction>()
        .init_resource::<InputFocus>()
        .insert_resource(InputFocusVisible(true))
        .add_systems(Update, sync);
        let fonts = kn_ui::konung_fonts(&mut app.world_mut().resource_mut::<Assets<Font>>());
        (app, fonts)
    }

    fn model(character: u32, count: usize, editable: bool) -> CharacterSheet {
        CharacterSheet {
            character,
            name: format!("Character {character}"),
            history: "A long synthetic biography. ".repeat(80),
            baseline_label: "Default".into(),
            groups: vec![StatGroup {
                placement: GroupPlacement::Statistics,
                label: "Attributes".into(),
                stats: (0..count)
                    .map(|index| Stat {
                        id: 1000 + index,
                        label: format!("Длинное название характеристики {index}"),
                        value: (125 + index).to_string(),
                        description: format!("Description {index}. ").repeat(50),
                        edit: editable.then(|| StatEdit {
                            baseline: "124".into(),
                            increase: StatOperation {
                                summary: "Increase: 5 points".into(),
                                blocked: Some("Not enough points".into()),
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
        }
    }

    #[test]
    fn inspection_supports_eight_characters_and_arbitrary_stat_counts_without_edit_controls() {
        let (mut app, fonts) = app();
        for character in 0..8 {
            let count = if character % 2 == 0 { 6 } else { 10 };
            let root = app
                .world_mut()
                .spawn_scene(sheet(&fonts, model(character, count, false), bsn_list! {}))
                .unwrap()
                .id();
            app.update();
            assert_eq!(
                app.world_mut()
                    .query::<&StatAction>()
                    .iter(app.world())
                    .count(),
                0
            );
            assert_eq!(
                app.world_mut()
                    .query::<&DetailSource>()
                    .iter(app.world())
                    .count(),
                count
            );
            let text = app
                .world_mut()
                .query::<&Text>()
                .iter(app.world())
                .map(|text| text.0.clone())
                .collect::<Vec<_>>();
            assert!(text.contains(&(124 + count).to_string()));
            assert!(
                !text
                    .iter()
                    .any(|text| text.contains("Increase:") || text == "+" || text == "−")
            );
            app.world_mut().despawn(root);
            app.update();
        }
    }

    #[test]
    fn focused_details_refresh_prices_and_pointer_selection_persists() {
        let (mut app, fonts) = app();
        let root = app
            .world_mut()
            .spawn_scene(sheet(&fonts, model(7, 10, true), bsn_list! {}))
            .unwrap()
            .id();
        app.update();
        let target = app
            .world_mut()
            .query::<(Entity, &DetailSource)>()
            .iter(app.world())
            .find(|(_, source)| source.0 == 1009)
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(target, FocusCause::Navigated);
        app.update();
        let detail = |app: &mut App| {
            app.world_mut()
                .query_filtered::<&Text, With<DetailText>>()
                .single(app.world())
                .unwrap()
                .0
                .clone()
        };
        assert!(detail(&mut app).contains("Description 9."));
        assert!(detail(&mut app).contains("Not enough points"));
        let mut data = model(7, 10, true);
        data.groups[0].stats[9]
            .edit
            .as_mut()
            .unwrap()
            .increase
            .summary = "Increase: 2 points".into();
        app.world_mut()
            .entity_mut(root)
            .insert(SheetModel(std::sync::Arc::new(data)));
        app.update();
        assert!(detail(&mut app).contains("Increase: 2 points"));
        let other = app
            .world_mut()
            .query::<(Entity, &DetailSource)>()
            .iter(app.world())
            .find(|(_, source)| source.0 == 1006)
            .unwrap()
            .0;
        app.world_mut().entity_mut(other).insert(Hovered(true));
        app.update();
        assert!(detail(&mut app).contains("Description 6."));
        app.world_mut().entity_mut(other).insert(Hovered(false));
        app.update();
        assert!(detail(&mut app).contains("Description 6."));
        let action = app
            .world_mut()
            .query::<(Entity, &StatAction, &bevy::ui::InteractionDisabled)>()
            .iter(app.world())
            .next()
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: action });
        app.update();
        assert_eq!(
            app.world().resource::<SheetInteraction>().selected,
            Some(1006)
        );
    }
}
