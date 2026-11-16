//! Contextual controls for the map workspace.
use crate::{
    action::{
        ActionQueue, DebugScope, InteractionMode, ReadProjection, ViewerAction, ViewerWorkspace,
    },
    *,
};
use bevy::{
    feathers::{
        controls::{
            FeathersButton, FeathersCheckbox, FeathersSelect, FeathersSlider, FeathersTextInput,
            FeathersTextInputContainer, OptionIndex, list_rows_from_strings,
        },
        display::caption,
        font_styles::InheritableFont,
        theme::{InheritableThemeTextColor, ThemeBackgroundColor},
        tokens,
    },
    input_focus::{InputFocus, InputFocusVisible},
    picking::{events::PointerScroll, hover::Hovered},
    prelude::*,
    text::EditableText,
    ui::{Checked, ScrollPosition},
    ui_widgets::{Activate, SliderStep, ValueChange},
};
use k1_game::time_of_day::CYCLE_TICKS;
use kn_ui::spacing::{PANEL_PADDING, SPACE_4, SPACE_8};

#[derive(Component, Clone, Default)]
pub(crate) struct WorkspaceViewport;
#[derive(Component, Clone, Copy, Default)]
enum ButtonAction {
    #[default]
    Select,
    Pan,
    DebugPlayer,
    RemovePlayer,
    ScopeSelected,
    ScopeAll,
    Fit,
    Retry,
    Play,
    Back,
    Forward,
    Display,
    Time,
    Diagnostics,
    Focus,
    ResetPositions,
    ResetAnimation,
}
#[derive(Component, Clone, Copy, Default)]
enum ToggleAction {
    #[default]
    Characters,
    Foot,
    Lights,
    Markers,
    ConstantDay,
    DebugRoutes,
    DebugStates,
    DebugRectangles,
}
#[derive(Component, Clone, Default)]
struct MapFilter;
#[derive(Component, Clone, Default)]
struct CreatureFilter;
#[derive(Component, Clone, Default)]
struct MapSelect;
#[derive(Component, Clone, Default)]
struct MapList;
#[derive(Component, Clone, Default)]
struct CreatureList;
#[derive(Component, Clone, Default)]
struct SpeedList;
#[derive(Resource, Default)]
struct Filtered {
    maps: Vec<usize>,
    creatures: Vec<usize>,
    key: String,
}
#[derive(Component, Clone, Default)]
struct CreatureSelect;
#[derive(Component, Clone, Default)]
struct Inspector;
#[derive(Component, Clone, Default)]
struct FinderStatus;
#[derive(Component, Clone, Default)]
struct Status;
#[derive(Component, Clone, Default)]
struct Timeline;
#[derive(Component, Clone, Default)]
struct DisplayDetails;
#[derive(Component, Clone, Default)]
struct TimeDetails;
#[derive(Component, Clone, Default)]
struct BottomBar;
#[derive(Component, Clone, Default)]
struct DiagnosticDetails;
#[derive(Component, Clone, Default)]
struct DiagnosticText;
#[derive(Component, Clone, Default)]
struct SelectLabel;
#[derive(Component, Clone, Default)]
struct PanLabel;
#[derive(Component, Clone, Default)]
struct ModeHint;
#[derive(Component, Clone, Default)]
struct DebugPlayerLabel;
#[derive(Component, Clone, Default)]
struct RemovePlayerButton;
#[derive(Component, Clone, Default)]
struct SelectedScopeLabel;
#[derive(Component, Clone, Default)]
struct AllScopeLabel;
#[derive(Component, Clone, Default)]
struct DebugScopeHint;
#[derive(Component, Clone, Default)]
struct DebugProfileError;

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Filtered>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                freeze_text_inputs,
                sync_inputs,
                sync_toggles,
                sync_debug_controls,
                sync_speed,
                publish_viewport.before(kn_render::CameraApply),
            ),
        )
        .add_systems(
            PostUpdate,
            reveal_display_focus.after(bevy::ui::UiSystems::PostLayout),
        )
        .add_systems(
            PostUpdate,
            sync_workspace
                .after(crate::action::project_state)
                .before(bevy::ui::UiSystems::Layout),
        );
}

fn setup(mut commands: Commands, viewer: Res<Viewer>, fonts: Res<kn_ui::KonungFonts>) {
    let _ = viewer;
    let ui_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 1,
                clear_color: bevy::camera::ClearColorConfig::None,
                ..default()
            },
            bevy::camera::visibility::RenderLayers::none(),
            bevy::ui::IsDefaultUiCamera,
        ))
        .id();
    commands.spawn_scene(bsn! {
        Node { width: percent(100), height: percent(100), max_height: percent(100), min_height: px(0), flex_direction: FlexDirection::Column }
        UiTargetCamera(ui_camera)
        InheritableFont { font: {fonts.regular.clone()}, font_size: kn_ui::typo::BODY, weight: bevy::text::FontWeight::NORMAL }
        InheritableThemeTextColor(tokens::TEXT_MAIN)
        Children[
            Node { height: px(48), min_height: px(48), padding: UiRect::all(SPACE_8), column_gap: SPACE_8, align_items: AlignItems::Center }
            Hovered ThemeBackgroundColor(tokens::WINDOW_BG)
            Children[
                @caption("Filter map: source + number")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersTextInputContainer Node { width: px(150), height: kn_ui::spacing::CONTROL_MIN_HEIGHT, min_height: kn_ui::spacing::CONTROL_MIN_HEIGHT, flex_grow: 0.0, flex_shrink: 0.0 } Children[
                    @FeathersTextInput MapFilter
                ]
                --
                Node { width: px(160) } MapList
                --
                @FeathersButton { @caption: bsn! { @caption("Display > (Esc closes)") } } ButtonAction::Display on(press)
                --
                @caption("") Node { flex_grow: 1.0, min_width: px(0), overflow: Overflow::clip() } Status
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersButton { @caption: bsn! { @caption("Retry") } } ButtonAction::Retry on(press)
            ]
            --
            Node { flex_grow: 1.0, flex_shrink: 1.0, flex_basis: px(0), min_height: px(0), flex_direction: FlexDirection::Row }
            Children[
                Node { width: px(112), min_width: px(112), padding: UiRect::all(PANEL_PADDING), flex_direction: FlexDirection::Column, row_gap: SPACE_8 }
                Hovered ThemeBackgroundColor(tokens::WINDOW_BG)
                Children[
                    @caption("TOOLS") InheritableFont { font: {fonts.bold.clone()}, font_size: kn_ui::typo::CAPTION, weight: bevy::text::FontWeight::BOLD }
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.bold.clone()}), font_size: kn_ui::typo::CAPTION }
                    --
                    @FeathersButton { @caption: bsn! { @caption("[x] Select") SelectLabel } } ButtonAction::Select on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("[ ] Pan") PanLabel } } ButtonAction::Pan on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("[ ] Player") DebugPlayerLabel } } ButtonAction::DebugPlayer on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("Remove") } } Node { display: Display::None } RemovePlayerButton ButtonAction::RemovePlayer on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("Fit (Home)") } } ButtonAction::Fit on(press)
                    --
                    @caption("Click a creature to inspect.") ModeHint
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                    --
                    @caption("") Node { width: percent(100), flex_shrink: 0.0 } DebugProfileError
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
                    TextColor(kn_ui::palette::DANGER)
                ]
                --
                Node { flex_grow: 1.0, min_width: px(0), min_height: px(0) } WorkspaceViewport
                --
                Node { width: px(260), min_width: px(220), height: percent(100), min_height: px(0), padding: UiRect::all(PANEL_PADDING), flex_direction: FlexDirection::Column, row_gap: SPACE_8, overflow: Overflow::scroll_y() }
                Hovered ScrollPosition::default() on(scroll) ThemeBackgroundColor(tokens::WINDOW_BG)
                Children[
                    @caption("INSPECTOR") InheritableFont { font: {fonts.bold.clone()}, font_size: kn_ui::typo::TITLE, weight: bevy::text::FontWeight::BOLD }
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.bold.clone()}), font_size: kn_ui::typo::TITLE }
                    --
                    @caption("Find creature by ID or name")
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                    --
                    @FeathersTextInputContainer Node { height: kn_ui::spacing::CONTROL_MIN_HEIGHT, min_height: kn_ui::spacing::CONTROL_MIN_HEIGHT, flex_grow: 0.0, flex_shrink: 0.0 } Children[ @FeathersTextInput CreatureFilter ]
                    --
                    Node { width: percent(100) } CreatureList
                    --
                    @caption("") FinderStatus
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
                    --
                    @FeathersButton { @caption: bsn! { @caption("Focus selection (F)") } } ButtonAction::Focus on(press)
                    --
                    @caption("Select a creature in the map or finder.") Node { width: percent(100), flex_shrink: 0.0 } Inspector
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                    --
                    @FeathersButton { @caption: bsn! { @caption("Diagnostics >") } } ButtonAction::Diagnostics on(press)
                    --
                    Node { display: Display::None, flex_direction: FlexDirection::Column } DiagnosticDetails
                    Children[ @caption("") Node { width: percent(100), flex_shrink: 0.0 } TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY } DiagnosticText ]
                ]
            ]
            --
            Node { height: px(52), min_height: px(52), padding: UiRect::all(SPACE_8), flex_direction: FlexDirection::Column, row_gap: SPACE_4, overflow: Overflow::clip() } BottomBar
            Hovered ThemeBackgroundColor(tokens::WINDOW_BG)
            Children[
                Node { flex_direction: FlexDirection::Row, column_gap: SPACE_8, align_items: AlignItems::Center }
                Children[
                    @FeathersButton { @caption: bsn! { @caption("Play / Pause (Space)") } } ButtonAction::Play on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("- tick (,)") } } ButtonAction::Back on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("+ tick (.)") } } ButtonAction::Forward on(press)
                    --
                    @caption("") Timeline
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                    --
                    @caption("Speed")
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                    --
                    Node { width: px(90), flex_shrink: 0.0 } SpeedList
                    --
                    @FeathersButton { @caption: bsn! { @caption("Seek simulation >") } } ButtonAction::Time on(press)
                ]
                --
                Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: SPACE_4 } TimeDetails
                Children[
                    @FeathersSlider { @min: 0.0, @max: {(CYCLE_TICKS - 1) as f32} } SliderStep(15.0) ScrubSlider on(change_scrub)
                    --
                    Node { flex_direction: FlexDirection::Row, column_gap: SPACE_8, flex_wrap: FlexWrap::Wrap } Children[
                        @FeathersButton { @caption: bsn! { @caption("Light 07:00") } } ScrubPreset(900) on(jump_to_preset)
                        --
                        @FeathersButton { @caption: bsn! { @caption("Warm 09:00") } } ScrubPreset(2700) on(jump_to_preset)
                        --
                        @FeathersButton { @caption: bsn! { @caption("Ground 18:13") } } ScrubPreset(11000) on(jump_to_preset)
                        --
                        @FeathersButton { @caption: bsn! { @caption("Ramp 03:04") } } ScrubPreset(18960) on(jump_to_preset)
                        --
                        @FeathersButton { @caption: bsn! { @caption("Reset positions") } } ButtonAction::ResetPositions on(press)
                        --
                        @FeathersButton { @caption: bsn! { @caption("Reset animation") } } ButtonAction::ResetAnimation on(press)
                    ]
                    --
                    @caption("Reset positions: return home. Reset animation: restart special animations; walk/rest, positions and clock stay unchanged.")
                    TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                ]
            ]
            --
            Node { position_type: PositionType::Absolute, top: px(48), left: px(330), max_height: percent(65), padding: UiRect::all(PANEL_PADDING), display: Display::None, flex_direction: FlexDirection::Column, row_gap: SPACE_4, overflow: Overflow::scroll_y() }
            Hovered ScrollPosition::default() on(scroll) ThemeBackgroundColor(tokens::WINDOW_BG) DisplayDetails
            Children[
                @caption("SCENE")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Characters") } } Checked ToggleAction::Characters on(toggle)
                --
                @caption("LIGHTING")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Constant day") } } ToggleAction::ConstantDay on(toggle)
                --
                @caption("Lighting only; simulation keeps running.")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
                --
                @caption("DIAGNOSTIC OVERLAYS")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Walkability") } } ToggleAction::Foot on(toggle)
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Lights") } } ToggleAction::Lights on(toggle)
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Waypoints") } } ToggleAction::Markers on(toggle)
                --
                @caption("SIMULATION DEBUG")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::BODY }
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Routes & targets") } } ToggleAction::DebugRoutes on(toggle)
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Group states") } } ToggleAction::DebugStates on(toggle)
                --
                @FeathersCheckbox { @caption: bsn! { @caption("Group rectangles") } } ToggleAction::DebugRectangles on(toggle)
                --
                @caption("Scope")
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
                --
                Node { flex_direction: FlexDirection::Row, column_gap: SPACE_4 } Children[
                    @FeathersButton { @caption: bsn! { @caption("[x] Selected") SelectedScopeLabel } } ButtonAction::ScopeSelected on(press)
                    --
                    @FeathersButton { @caption: bsn! { @caption("[ ] All") AllScopeLabel } } ButtonAction::ScopeAll on(press)
                ]
                --
                @caption("") DebugScopeHint
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
                --
                @caption("Rectangles: spawn area and second rectangle (raw data).")
                Node { width: px(220) }
                TextFont { font: bevy::text::FontSourceTemplate::Handle({fonts.regular.clone()}), font_size: kn_ui::typo::CAPTION }
            ]
        ]
    });
}

fn press(
    event: On<Activate>,
    buttons: Query<&ButtonAction>,
    mut queue: ResMut<ActionQueue>,
    mut workspace: ResMut<ViewerWorkspace>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    let Ok(button) = buttons.get(event.entity) else {
        return;
    };
    let action = match button {
        ButtonAction::Display => {
            workspace.display_open = !workspace.display_open;
            return;
        }
        ButtonAction::Time => {
            workspace.time_open = !workspace.time_open;
            return;
        }
        ButtonAction::Diagnostics => {
            workspace.diagnostics_open = !workspace.diagnostics_open;
            return;
        }
        ButtonAction::Select => ViewerAction::Mode {
            mode: InteractionMode::Select,
        },
        ButtonAction::Pan => ViewerAction::Mode {
            mode: InteractionMode::Pan,
        },
        ButtonAction::DebugPlayer => ViewerAction::Mode {
            mode: InteractionMode::DebugPlayer,
        },
        ButtonAction::RemovePlayer => ViewerAction::DebugRemove {},
        ButtonAction::ScopeSelected => ViewerAction::DebugScope {
            scope: DebugScope::Selected,
        },
        ButtonAction::ScopeAll => ViewerAction::DebugScope {
            scope: DebugScope::All,
        },
        ButtonAction::Fit => ViewerAction::FitMap {},
        ButtonAction::Retry => ViewerAction::RetryMap {},
        ButtonAction::Play => ViewerAction::TogglePlayback {},
        ButtonAction::Back => ViewerAction::Step { ticks: -1 },
        ButtonAction::Forward => ViewerAction::Step { ticks: 1 },
        ButtonAction::Focus => ViewerAction::FocusSelection {},
        ButtonAction::ResetPositions => ViewerAction::ResetPositions {},
        ButtonAction::ResetAnimation => ViewerAction::ResetAnimation {},
    };
    queue.0.push_back(action);
}
fn toggle(
    event: On<ValueChange<bool>>,
    controls: Query<&ToggleAction>,
    mut queue: ResMut<ActionQueue>,
    mut commands: Commands,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    let Ok(control) = controls.get(event.source) else {
        return;
    };
    kn_ui::sync_checked(&mut commands, event.source, event.value);
    let action = match control {
        ToggleAction::ConstantDay => ViewerAction::ConstantDay {
            enabled: event.value,
        },
        ToggleAction::DebugRoutes => {
            for layer in ["routes", "targets"] {
                queue.0.push_back(ViewerAction::DebugDisplay {
                    layer: layer.into(),
                    visible: event.value,
                });
            }
            return;
        }
        ToggleAction::DebugStates => ViewerAction::DebugDisplay {
            layer: "group_states".into(),
            visible: event.value,
        },
        ToggleAction::DebugRectangles => ViewerAction::DebugDisplay {
            layer: "regions".into(),
            visible: event.value,
        },
        other => ViewerAction::Display {
            layer: match other {
                ToggleAction::Characters => "characters",
                ToggleAction::Foot => "foot",
                ToggleAction::Lights => "lights",
                ToggleAction::Markers => "markers",
                ToggleAction::ConstantDay
                | ToggleAction::DebugRoutes
                | ToggleAction::DebugStates
                | ToggleAction::DebugRectangles => unreachable!(),
            }
            .into(),
            visible: event.value,
        },
    };
    queue.0.push_back(action);
}
fn select_map(
    event: On<ValueChange<Entity>>,
    options: Query<&OptionIndex>,
    filtered: Res<Filtered>,
    viewer: Res<Viewer>,
    mut queue: ResMut<ActionQueue>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    if let Ok(option) = options.get(event.value)
        && let Some(map) = filtered
            .maps
            .get(option.0)
            .and_then(|index| viewer.maps.get(*index))
    {
        queue.0.push_back(ViewerAction::Map {
            source: map.source.to_string().to_lowercase(),
            number: map.number,
        });
    }
}
fn select_creature(
    event: On<ValueChange<Entity>>,
    options: Query<&OptionIndex>,
    filtered: Res<Filtered>,
    mut queue: ResMut<ActionQueue>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    if let Ok(option) = options.get(event.value)
        && let Some(id) = filtered.creatures.get(option.0)
    {
        queue.0.push_back(ViewerAction::Select { id: Some(*id) });
    }
}
fn scroll(
    mut event: On<PointerScroll>,
    mut nodes: Query<(&mut ScrollPosition, &ComputedNode)>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        event.propagate(false);
        return;
    }
    if let Ok((mut position, node)) = nodes.get_mut(event.entity) {
        let factor = match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => 32.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
        };
        let maximum = (node.content_size.y - node.size.y).max(0.0) * node.inverse_scale_factor;
        position.y = (position.y - event.y * factor).clamp(0.0, maximum);
        event.propagate(false);
    }
}
fn reveal_display_focus(
    focus: Res<InputFocus>,
    visible: Res<InputFocusVisible>,
    control: Option<Res<crate::control::ControlServer>>,
    mut last_focus: Local<Option<Entity>>,
    parents: Query<&ChildOf>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut display: Query<
        (
            Entity,
            &ComputedNode,
            &UiGlobalTransform,
            &mut ScrollPosition,
        ),
        With<DisplayDetails>,
    >,
) {
    if capturing(&control) {
        return;
    }
    let focused = focus.get();
    if *last_focus == focused {
        return;
    }
    *last_focus = focused;
    if !visible.0 {
        return;
    }
    let Some(target) = focused else {
        return;
    };
    let Ok((display_entity, viewport, viewport_transform, mut position)) = display.single_mut()
    else {
        return;
    };
    let mut child = target;
    while child != display_entity {
        let Ok(parent) = parents.get(child) else {
            return;
        };
        child = parent.parent();
    }
    let Ok((node, transform)) = nodes.get(target) else {
        return;
    };
    if node.size.y <= 0.0 || viewport.size.y <= 0.0 {
        return;
    }
    let target_center = transform.affine().translation.y;
    let viewport_center = viewport_transform.affine().translation.y;
    let target_min = target_center - node.size.y / 2.0;
    let target_max = target_center + node.size.y / 2.0;
    let viewport_min = viewport_center - viewport.size.y / 2.0;
    let viewport_max = viewport_center + viewport.size.y / 2.0;
    let delta = if target_min < viewport_min {
        target_min - viewport_min
    } else if target_max > viewport_max {
        target_max - viewport_max
    } else {
        0.0
    };
    let scale = viewport.inverse_scale_factor;
    let maximum = (viewport.content_size.y - viewport.size.y).max(0.0) * scale;
    position.y = (position.y + delta * scale).clamp(0.0, maximum);
}

fn sync_inputs(
    map: Query<(Entity, &EditableText), With<MapFilter>>,
    creature: Query<(Entity, &EditableText), With<CreatureFilter>>,
    focus: Res<InputFocus>,
    mut workspace: ResMut<ViewerWorkspace>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    if let Ok((_, text)) = map.single() {
        workspace.map_search = text.value().to_string();
    }
    if let Ok((_, text)) = creature.single() {
        workspace.creature_search = text.value().to_string();
    }
    workspace.keyboard_captured = focus
        .get()
        .is_some_and(|entity| map.get(entity).is_ok() || creature.get(entity).is_ok());
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_workspace(
    mut commands: Commands,
    workspace: Res<ViewerWorkspace>,
    projection: Res<ReadProjection>,
    viewer: Res<Viewer>,
    time: Res<Time>,
    control: Option<Res<crate::control::ControlServer>>,
    mut notice: Local<(u64, f32)>,
    mut filtered: ResMut<Filtered>,
    maps: Query<Entity, With<MapList>>,
    creatures: Query<Entity, With<CreatureList>>,
    mut text: ParamSet<(
        Query<&mut Text, With<Status>>,
        Query<&mut Text, With<Inspector>>,
        Query<&mut Text, With<Timeline>>,
        Query<&mut Text, With<DiagnosticText>>,
        Query<&mut Text, With<SelectLabel>>,
        Query<&mut Text, With<PanLabel>>,
        Query<&mut Text, With<ModeHint>>,
        Query<&mut Text, With<FinderStatus>>,
    )>,
    mut details: ParamSet<(
        Query<&mut Node, With<DisplayDetails>>,
        Query<&mut Node, With<TimeDetails>>,
        Query<&mut Node, With<DiagnosticDetails>>,
        Query<&mut Node, With<BottomBar>>,
    )>,
) {
    if !capturing(&control) && notice.0 != workspace.feedback_revision {
        notice.0 = workspace.feedback_revision;
        notice.1 = 4.0;
    }
    if !capturing(&control) {
        notice.1 = (notice.1 - time.delta_secs()).max(0.0);
    }
    let data = &projection.0;
    let list = data["creatures"].as_array().cloned().unwrap_or_default();
    let labels_key: Vec<_> = list.iter().map(|c| (&c["id"], &c["label"])).collect();
    let key = format!(
        "{}|{}|{:?}|{:?}",
        workspace.map_search, workspace.creature_search, viewer.selected, labels_key
    );
    if filtered.key != key {
        filtered.key = key;
        let query = workspace.map_search.to_lowercase();
        let labels = map_labels(&viewer.maps);
        filtered.maps = labels
            .iter()
            .enumerate()
            .filter(|(i, label)| {
                label.to_lowercase().contains(&query)
                    || format!("{} {}", viewer.maps[*i].source, viewer.maps[*i].number)
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        let visible: Vec<_> = filtered.maps.iter().map(|i| labels[*i].clone()).collect();
        let selected = filtered
            .maps
            .iter()
            .position(|index| *index == viewer.selected);
        if let Ok(parent) = maps.single() {
            commands.entity(parent).despawn_children();
            commands.spawn_scene(bsn! { @FeathersSelect { @options: list_rows_from_strings(visible, selected), @max_visible: 12 } Node { width: percent(100) } MapSelect on(select_map) }).insert(ChildOf(parent));
        }
        let query = workspace.creature_search.to_lowercase();
        let matches: Vec<_> = list
            .iter()
            .filter_map(|c| {
                let id = c["id"].as_u64()? as usize;
                let label = c["label"].as_str().unwrap_or("").to_string();
                label.to_lowercase().contains(&query).then_some((id, label))
            })
            .collect();
        filtered.creatures = matches.iter().map(|(id, _)| *id).collect();
        if let Ok(parent) = creatures.single() {
            commands.entity(parent).despawn_children();
            let labels: Vec<_> = matches.into_iter().map(|(_, label)| label).collect();
            commands.spawn_scene(bsn! { @FeathersSelect { @options: list_rows_from_strings(labels, None), @max_visible: 10 } Node { width: percent(100) } CreatureSelect on(select_creature) }).insert(ChildOf(parent));
        }
    }
    if let Ok(mut t) = text.p0().single_mut() {
        **t = if let Some(error) = data["error"].as_str() {
            format!(
                "ERROR: {error} | requested {} | displayed {}",
                map_name(&data["requested_map"]),
                map_name(&data["displayed_map"])
            )
        } else if let Some(error) = data["simulation_error"].as_str() {
            format!("SIMULATION ERROR: {error}")
        } else if data["phase"] == "seeking" {
            format!(
                "Seeking: simulation tick {} / target {}",
                data["simulation_tick"], data["tick"]
            )
        } else if data["phase"] == "lighting_preview" {
            format!(
                "Lighting preview at tick {}; release to update creatures",
                data["preview_tick"]
            )
        } else if filtered.maps.is_empty() {
            "No matching maps".into()
        } else {
            if notice.1 <= 0.0 || workspace.feedback.is_empty() || workspace.feedback == "Applied" {
                format!(
                    "{} | {:?}",
                    map_name(&data["displayed_map"]),
                    workspace.mode
                )
            } else {
                workspace.feedback.clone()
            }
        };
    }
    if let Ok(mut t) = text.p1().single_mut() {
        **t = if let Some(error) = data["error"].as_str() {
            format!(
                "MAP LOAD ERROR\n{error}\nRequested: {}\nDisplayed: {}\nUse Retry above.",
                map_name(&data["requested_map"]),
                map_name(&data["displayed_map"])
            )
        } else if let Some(error) = data["simulation_error"].as_str() {
            format!("SIMULATION ERROR\n{error}\nThe scene may be out of sync.")
        } else if data["displayed_map"].is_null() {
            "No map displayed.".into()
        } else if !data["simulation_available"].as_bool().unwrap_or(false) {
            format!(
                "{}\nSimulation unavailable: {}",
                map_name(&data["displayed_map"]),
                data["simulation_error"]
                    .as_str()
                    .unwrap_or("not initialized")
            )
        } else if let Some(id) = workspace.selected_creature {
            list.iter()
                .find(|c| c["id"].as_u64() == Some(id as u64))
                .map_or_else(
                    || "Selection unavailable.".into(),
                    |c| {
                        let mut details = format!(
                            "{}{}\nRecorded state: {}\nAnimation state: {} ({})\nRendered: {}\nGroup: {}\nGroup state: {} - {}\nGroup tick: {}",
                            c["label"].as_str().unwrap_or(""),
                            if viewer.overlays[3] { "" } else { " (hidden)" },
                            c["recorded_state"],
                            friendly(&c["animation_state"]), friendly(&c["animation_stage"]),
                            render_layer_summary(&data["selected_render_layers"]), c["group"],
                            friendly(&c["group_state_value"]), friendly(&c["group_state"]),
                            if c["group_state_tick"].is_null() { "Unevaluated".into() } else { friendly(&c["group_state_tick"]) }
                        );
                        if !c["attack"].is_null() {
                            details.push_str(&format!("\nAttack: {}\nAttack target: {}", friendly(&c["attack"]["status"]), friendly(&c["attack"]["target_id"])));
                            if !c["attack"]["error"].is_null() {
                                details.push_str(&format!("\nAttack refusal: {}", friendly(&c["attack"]["error"])));
                            }
                        }
                        if !c["pursuit"].is_null() {
                            details.push_str(&format!("\nPursuit: {}\nTarget: Debug player #{}\nTarget cell: {}", friendly(&c["pursuit"]["status"]), friendly(&c["pursuit"]["target_id"]), friendly(&c["pursuit"]["target_cell"])));
                            if !c["pursuit"]["error"].is_null() {
                                details.push_str(&format!("\nPursuit refusal: {}", friendly(&c["pursuit"]["error"])));
                            }
                        }
                        if !c["animation_melee_frame"].is_null() {
                            details.push_str(&format!("\nMelee frame: {}", friendly(&c["animation_melee_frame"])));
                        }
                        if c["cell"].is_null() { details.push_str("\nNot simulated"); }
                        else { details.push_str(&format!("\nMovement: {}\nCell: {}\nPixel: {}\nDirection: {}", friendly(&c["walk_state"]), friendly(&c["cell"]), friendly(&c["pixel"]), friendly(&c["direction"]))); }
                        details
                    },
                )
        } else if list.is_empty() {
            format!(
                "{}\nNo creatures on this map.",
                map_name(&data["displayed_map"])
            )
        } else {
            format!(
                "{}\n{} creatures | {} textures\n{}",
                map_name(&data["displayed_map"]),
                list.len(),
                data["map_facts"]["unique_textures"],
                if !viewer.overlays[3] {
                    "Characters hidden; enable them in Display."
                } else if filtered.creatures.is_empty() {
                    "No matching creatures."
                } else {
                    "Select a creature in the map or finder."
                }
            )
        };
    }

    if let Ok(mut t) = text.p2().single_mut() {
        let tick = data["tick"].as_u64().unwrap_or(0);
        let clock = k1_game::time_of_day::game_time(tick);
        **t = format!(
            "{} | tick {} | {:02}:{:02} {}",
            if data["playing"] == true {
                "Playing"
            } else {
                "Paused"
            },
            tick,
            clock.clock_hour,
            clock.clock_minute,
            phase_label(clock.phase)
        );
        if data["phase"] == "seeking" {
            t.push_str(&format!(
                " | seeking {} / {}",
                data["simulation_tick"], tick
            ));
        } else if data["phase"] == "lighting_preview" {
            t.push_str(" | preview; release to update creatures");
        } else if let Some(sim_tick) = data["simulation_tick"].as_u64() {
            t.push_str(&format!(" | sim {sim_tick}"));
        }
    }
    if let Ok(mut t) = text.p3().single_mut() {
        **t = format!(
            "Requested: {}\nDisplayed: {}\nCamera: {}\nZoom: {}\nPieces: {} built / {} visible\nFPS: {}\nTexture cache: {} / {} unique\nAnimations: {} pieces / {} frames\nBuild: {} ms\nTexture problems: {}\nLayers: {}\n{}",
            map_name(&data["requested_map"]),
            map_name(&data["displayed_map"]),
            data["camera"],
            data["zoom"],
            data["diagnostics"]["pieces_built"],
            data["diagnostics"]["pieces_visible"],
            data["diagnostics"]["fps"],
            data["map_facts"]["cached_textures"],
            data["map_facts"]["unique_textures"],
            data["map_facts"]["animated_pieces"],
            data["map_facts"]["animation_frames"],
            data["map_facts"]["built_ms"],
            data["map_facts"]["texture_problems"],
            render_layers(&data["selected_render_layers"]),
            workspace.feedback
        );
    }
    if let Ok(mut t) = text.p4().single_mut() {
        **t = if workspace.mode == InteractionMode::Select {
            "[x] Select"
        } else {
            "[ ] Select"
        }
        .into();
    }
    if let Ok(mut t) = text.p5().single_mut() {
        **t = if workspace.mode == InteractionMode::Pan {
            "[x] Pan"
        } else {
            "[ ] Pan"
        }
        .into();
    }
    if let Ok(mut t) = text.p6().single_mut() {
        **t = match workspace.mode {
            InteractionMode::Select => "Click a creature to inspect.",
            InteractionMode::Pan => "Drag to move the map.",
            InteractionMode::DebugPlayer if data["debug_player"].is_null() => {
                "Click a free walkable cell to place the debug player."
            }
            InteractionMode::DebugPlayer => "Click a destination to move the debug player.",
        }
        .into();
    }
    if let Ok(mut t) = text.p7().single_mut() {
        **t = if workspace.creature_search.is_empty() {
            String::new()
        } else if filtered.creatures.is_empty() {
            "No matching creatures.".into()
        } else {
            format!("{} matching creatures", filtered.creatures.len())
        };
    }
    if let Ok(mut n) = details.p0().single_mut() {
        n.display = if workspace.display_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut n) = details.p1().single_mut() {
        n.display = if workspace.time_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut n) = details.p2().single_mut() {
        n.display = if workspace.diagnostics_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut n) = details.p3().single_mut() {
        let height = if workspace.time_open { px(144) } else { px(52) };
        n.height = height;
        n.min_height = height;
    }
}
fn publish_viewport(
    window: Query<&Window>,
    workspace: Res<ViewerWorkspace>,
    mut viewport: ResMut<kn_render::MapViewport>,
) {
    if let Ok(window) = window.single() {
        let bottom = if workspace.time_open { 144.0 } else { 52.0 };
        viewport.logical_rect = Some(Rect::from_corners(
            Vec2::new(112.0, 48.0),
            Vec2::new(
                (window.width() - 260.0).max(113.0),
                (window.height() - bottom).max(49.0),
            ),
        ));
        viewport.scale_factor = window.scale_factor();
    }
}

fn sync_toggles(
    mut commands: Commands,
    viewer: Res<Viewer>,
    workspace: Res<ViewerWorkspace>,
    controls: Query<(Entity, &ToggleAction, Has<Checked>)>,
) {
    for (entity, control, checked) in &controls {
        let want = match control {
            ToggleAction::Characters => viewer.overlays[3],
            ToggleAction::Foot => viewer.overlays[0],
            ToggleAction::Lights => viewer.overlays[1],
            ToggleAction::Markers => viewer.overlays[2],
            ToggleAction::ConstantDay => viewer.constant_day,
            ToggleAction::DebugRoutes => workspace.debug_overlays[0] && workspace.debug_overlays[1],
            ToggleAction::DebugStates => workspace.debug_overlays[2],
            ToggleAction::DebugRectangles => workspace.debug_overlays[3],
        };
        if checked != want {
            kn_ui::sync_checked(&mut commands, entity, want);
        }
    }
}

#[allow(clippy::type_complexity)]
fn sync_debug_controls(
    workspace: Res<ViewerWorkspace>,
    projection: Res<ReadProjection>,
    mut text: ParamSet<(
        Query<&mut Text, With<DebugPlayerLabel>>,
        Query<&mut Text, With<SelectedScopeLabel>>,
        Query<&mut Text, With<AllScopeLabel>>,
        Query<&mut Text, With<DebugScopeHint>>,
        Query<&mut Text, With<DebugProfileError>>,
    )>,
    mut remove: Query<&mut Node, With<RemovePlayerButton>>,
) {
    if let Ok(mut label) = text.p0().single_mut() {
        **label = if workspace.mode == InteractionMode::DebugPlayer {
            "[x] Player"
        } else {
            "[ ] Player"
        }
        .into();
    }
    if let Ok(mut label) = text.p1().single_mut() {
        **label = if workspace.debug_scope == DebugScope::Selected {
            "[x] Selected"
        } else {
            "[ ] Selected"
        }
        .into();
    }
    if let Ok(mut label) = text.p2().single_mut() {
        **label = if workspace.debug_scope == DebugScope::All {
            "[x] All"
        } else {
            "[ ] All"
        }
        .into();
    }
    if let Ok(mut hint) = text.p3().single_mut() {
        **hint = if workspace.debug_scope == DebugScope::Selected
            && workspace.selected_creature.is_none()
        {
            "Select a creature for squad details, or choose All."
        } else {
            ""
        }
        .into();
    }
    if let Ok(mut error) = text.p4().single_mut() {
        **error = if workspace.mode == InteractionMode::DebugPlayer {
            projection.0["debug"]["profile_error"]
                .as_str()
                .map(|message| format!("Player unavailable: {message}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
    }
    if let Ok(mut node) = remove.single_mut() {
        node.display = if projection.0["debug_player"].is_null() {
            Display::None
        } else {
            Display::Flex
        };
    }
}

fn map_name(value: &serde_json::Value) -> String {
    let Some(number) = value["number"].as_u64() else {
        return "None".into();
    };
    let source = match value["source"].as_str() {
        Some("game") => "Map",
        Some("tutorial") => "Tutorial",
        _ => "Map",
    };
    format!("{source} {number}")
}

fn capturing(control: &Option<Res<crate::control::ControlServer>>) -> bool {
    control.as_ref().is_some_and(|server| server.capturing())
}

#[allow(clippy::type_complexity)]
fn freeze_text_inputs(
    mut commands: Commands,
    control: Option<Res<crate::control::ControlServer>>,
    inputs: Query<
        (Entity, Has<bevy::ui::InteractionDisabled>),
        Or<(With<MapFilter>, With<CreatureFilter>)>,
    >,
) {
    let frozen = capturing(&control);
    for (entity, disabled) in &inputs {
        if frozen && !disabled {
            commands
                .entity(entity)
                .insert(bevy::ui::InteractionDisabled);
        } else if !frozen && disabled {
            commands
                .entity(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        }
    }
}

fn render_layer_summary(layers: &serde_json::Value) -> String {
    layers
        .as_array()
        .and_then(|items| items.first())
        .map(|layer| {
            format!(
                "{} frame {}",
                layer["clip"].as_str().unwrap_or("unknown"),
                layer["frame"]
            )
        })
        .unwrap_or_else(|| "unavailable".into())
}
fn render_layers(layers: &serde_json::Value) -> String {
    layers
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|layer| {
                    format!(
                        "{}: {} frame {} ({})",
                        layer["layer"],
                        layer["clip"].as_str().unwrap_or("unknown"),
                        layer["frame"],
                        if layer["visible"] == true {
                            "visible"
                        } else {
                            "hidden"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".into())
}

fn friendly(value: &serde_json::Value) -> String {
    if value.is_null() {
        return "Unavailable".into();
    }
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let raw = raw
        .strip_prefix("Some(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&raw);
    if raw == "None" {
        return "Unavailable".into();
    }
    let mut out = String::new();
    for (index, character) in raw.chars().enumerate() {
        if character == '_' {
            out.push(' ');
            continue;
        }
        if index > 0 && character.is_uppercase() {
            out.push(' ');
        }
        out.push(character);
    }
    out
}

const SPEEDS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
fn select_speed(
    event: On<ValueChange<Entity>>,
    options: Query<&OptionIndex>,
    mut queue: ResMut<ActionQueue>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    if let Ok(option) = options.get(event.value)
        && let Some(rate) = SPEEDS.get(option.0)
    {
        queue.0.push_back(ViewerAction::Speed { rate: *rate });
    }
}
fn sync_speed(
    mut commands: Commands,
    projection: Res<ReadProjection>,
    parent: Query<Entity, With<SpeedList>>,
    mut previous: Local<Option<f32>>,
    control: Option<Res<crate::control::ControlServer>>,
) {
    if capturing(&control) {
        return;
    }
    let speed = projection.0["speed"].as_f64().unwrap_or(1.0) as f32;
    if *previous == Some(speed) {
        return;
    }
    let Ok(parent) = parent.single() else {
        return;
    };
    *previous = Some(speed);
    let selected = SPEEDS.iter().position(|value| *value == speed);
    commands.entity(parent).despawn_children();
    commands.spawn_scene(bsn! { @FeathersSelect { @options: list_rows_from_strings(["0.25x", "0.5x", "1x", "2x", "4x", "8x"], selected), @max_visible: 6 } Node { width: percent(100) } on(select_speed) }).insert(ChildOf(parent));
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use bevy::{
        camera::{ComputedCameraValues, RenderTargetInfo, Viewport},
        input::mouse::MouseScrollUnit,
    };
    use serde_json::json;

    fn app(width: u32, height: u32) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default())
            .init_asset::<bevy::shader::Shader>()
            .add_plugins(DefaultPlugins.build()
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
                .set(WindowPlugin { primary_window: Some(Window { resolution: (width, height).into(), ..default() }), ..default() }))
            .add_plugins(kn_ui::KonungUiPlugin)
            .insert_resource(Viewer { maps: vec![kn_game::MapRef { source: kn_game::MapSource::Game, number: 17 }], selected: 0, overlays: DEFAULT_OVERLAYS, playback: DEFAULT_PLAYBACK, constant_day: false })
            .init_resource::<ViewerWorkspace>()
            .init_resource::<ActionQueue>()
            .init_resource::<kn_render::MapViewport>()
            .insert_resource(ReadProjection(json!({"displayed_map":{"source":"game","number":17}, "requested_map":{"source":"game","number":17}, "simulation_available":true, "map_facts":{"unique_textures":42}, "creatures":[], "tick":0, "time":"07:00", "playing":true})));
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(width, height),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                viewport: Some(Viewport {
                    physical_size: UVec2::new(width, height),
                    ..default()
                }),
                ..default()
            },
        ));
        install(&mut app);
        app.finish();
        app.cleanup();
        app.update();
        let ui_camera = app
            .world_mut()
            .query_filtered::<Entity, With<bevy::ui::IsDefaultUiCamera>>()
            .single(app.world())
            .unwrap();
        let mut camera = app.world_mut().get_mut::<Camera>(ui_camera).unwrap();
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size: UVec2::new(width, height),
            scale_factor: 1.0,
        });
        camera.viewport = Some(Viewport {
            physical_size: UVec2::new(width, height),
            ..default()
        });
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
    fn only<T: Component>(app: &mut App) -> Entity {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .single(app.world())
            .unwrap()
    }
    fn text_entity(app: &mut App, label: &str) -> Entity {
        app.world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .find(|(_, text)| text.0 == label)
            .unwrap_or_else(|| panic!("missing {label}"))
            .0
    }
    fn inside(inner: Rect, outer: Rect) -> bool {
        inner.min.x >= outer.min.x - 0.5
            && inner.max.x <= outer.max.x + 0.5
            && inner.min.y >= outer.min.y - 0.5
            && inner.max.y <= outer.max.y + 0.5
    }
    #[test]
    fn workspace_layout_keeps_viewport_and_expanded_controls_visible() {
        for (width, height) in [(1024, 720), (1280, 800), (960, 640)] {
            let mut app = app(width, height);
            {
                let mut state = app.world_mut().resource_mut::<ViewerWorkspace>();
                state.time_open = true;
                state.display_open = true;
            }
            for _ in 0..4 {
                app.update();
            }
            let theme = app.world().resource::<bevy::feathers::theme::UiTheme>();
            assert_eq!(theme.color(&tokens::TEXT_INPUT_TEXT), kn_ui::palette::TEXT);
            assert_eq!(theme.color(&tokens::LISTROW_TEXT), kn_ui::palette::TEXT);
            let filter = only::<CreatureFilter>(&mut app);
            assert_eq!(
                app.world().get::<TextColor>(filter).unwrap().0,
                kn_ui::palette::TEXT
            );
            let inspector_text = only::<Inspector>(&mut app);
            let font = app.world().get::<TextFont>(inspector_text).unwrap();
            assert_eq!(font.font_size, kn_ui::typo::BODY);
            assert_eq!(
                font.font,
                bevy::text::FontSource::Handle(
                    app.world().resource::<kn_ui::KonungFonts>().regular.clone()
                )
            );
            let viewport = only::<WorkspaceViewport>(&mut app);
            let bottom = only::<BottomBar>(&mut app);
            let display = only::<DisplayDetails>(&mut app);
            let viewport_bounds = bounds(app.world(), viewport);
            let bottom_bounds = bounds(app.world(), bottom);
            let display_bounds = bounds(app.world(), display);
            let window = Rect::from_corners(Vec2::ZERO, Vec2::new(width as f32, height as f32));
            assert!(
                inside(viewport_bounds, window)
                    && viewport_bounds.width() > 200.0
                    && viewport_bounds.height() > 200.0,
                "viewport {width}x{height}: {viewport_bounds:?}"
            );
            assert!(
                viewport_bounds.max.y <= bottom_bounds.min.y + 0.5,
                "viewport overlaps bottom bar"
            );
            assert!(
                inside(display_bounds, window),
                "display disclosure outside window: {display_bounds:?}"
            );
            assert!(
                display_bounds.max.y <= bottom_bounds.min.y + 0.5,
                "display disclosure overlaps transport: {display_bounds:?} vs {bottom_bounds:?}"
            );
            for label in [
                "Play / Pause (Space)",
                "- tick (,)",
                "+ tick (.)",
                "Seek simulation >",
                "Speed",
                "Light 07:00",
                "Warm 09:00",
                "Ground 18:13",
                "Ramp 03:04",
                "Reset positions",
                "Reset animation",
                "Reset positions: return home. Reset animation: restart special animations; walk/rest, positions and clock stay unchanged.",
            ] {
                let entity = text_entity(&mut app, label);
                let rect = bounds(app.world(), entity);
                assert!(
                    rect.width() > 0.0 && rect.height() > 0.0 && inside(rect, bottom_bounds),
                    "{label} clipped at {width}x{height}: {rect:?} outside {bottom_bounds:?}"
                );
            }
            let lighting = text_entity(&mut app, "Constant day");
            assert!(
                inside(bounds(app.world(), lighting), display_bounds),
                "lighting setting should be in Display"
            );
            let logical = app
                .world()
                .resource::<kn_render::MapViewport>()
                .logical_rect
                .unwrap();
            assert!(
                (logical.min - viewport_bounds.min).length() < 1.0
                    && (logical.max - viewport_bounds.max).length() < 1.0,
                "camera viewport differs from layout: {logical:?} vs {viewport_bounds:?}"
            );
        }
    }
    #[test]
    fn inspector_explains_the_attack_target_phase_and_refusal() {
        let mut app = app(960, 640);
        app.world_mut().resource_mut::<ReadProjection>().0 = json!({
            "displayed_map":{"source":"game","number":17},
            "simulation_available":true,
            "creatures":[{
                "id":4,"label":"Synthetic attacker","recorded_state":0,
                "animation_state":5,"animation_stage":"melee","animation_melee_frame":3,
                "group":1,"group_state":"player_inside","group_state_tick":20,
                "attack":{"target_id":41,"status":"refused","error":"no_route"}
            }]
        });
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = Some(4);
        app.update();
        let inspector = only::<Inspector>(&mut app);
        let text = &app.world().get::<Text>(inspector).unwrap().0;
        for detail in [
            "Attack: refused",
            "Attack target: 41",
            "Attack refusal: no route",
            "Melee frame: 3",
        ] {
            assert!(text.contains(detail), "missing {detail:?}: {text}");
        }
    }

    #[test]
    fn inspector_distinguishes_runtime_pursuit_from_an_attack() {
        let mut app = app(960, 640);
        app.world_mut().resource_mut::<ReadProjection>().0 = json!({
            "displayed_map":{"source":"game","number":17},
            "simulation_available":true,
            "creatures":[{
                "id":4,"label":"Synthetic pursuer","recorded_state":0,
                "animation_state":0,"animation_stage":"idle",
                "group":1,"group_state":"player_inside","group_state_tick":20,
                "pursuit":{"target_id":7,"target_cell":[10,12],"status":"refused","error":"no_route"}
            }]
        });
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = Some(4);
        app.update();
        let inspector = only::<Inspector>(&mut app);
        let text = &app.world().get::<Text>(inspector).unwrap().0;
        for detail in [
            "Pursuit: refused",
            "Target: Debug player #7",
            "Pursuit refusal: no route",
        ] {
            assert!(text.contains(detail), "missing {detail:?}: {text}");
        }
        assert!(!text.contains("Attack:"));
    }

    #[test]
    fn inspector_scrolls_long_details_without_expanding_the_viewport() {
        let mut app = app(960, 640);
        let long = "Synthetic creature with a very long name ".repeat(100);
        app.world_mut().resource_mut::<ReadProjection>().0 = json!({"displayed_map":{"source":"game","number":17}, "simulation_available":true, "creatures":[{"id":4,"label":long,"recorded_state":1,"animation_state":2,"animation_stage":"Standing","walk_state":"Idle","group":1,"group_state":"Idle","group_state_tick":0}]});
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = Some(4);
        for _ in 0..5 {
            app.update();
        }
        let inspector = only::<Inspector>(&mut app);
        let scroll = app.world().get::<ChildOf>(inspector).unwrap().parent();
        let node = app.world().get::<ComputedNode>(scroll).unwrap();
        assert!(
            node.content_size.y > node.size.y,
            "long inspector should overflow its scroll region"
        );
        let viewport = only::<WorkspaceViewport>(&mut app);
        assert!(bounds(app.world(), viewport).height() > 200.0);
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .single(app.world())
            .unwrap();
        app.world_mut().trigger(PointerScroll {
            entity: scroll,
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
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -5.0,
            hit: bevy::picking::backend::HitData::new(window, 0.0, None, None),
            phase: bevy::input::touch::TouchPhase::Moved,
        });
        app.update();
        assert!(app.world().get::<ScrollPosition>(scroll).unwrap().y > 0.0);
    }
    #[test]
    fn debug_controls_follow_player_and_scope_without_clearing_selection() {
        let mut app = app(1024, 720);
        {
            let mut workspace = app.world_mut().resource_mut::<ViewerWorkspace>();
            workspace.mode = InteractionMode::DebugPlayer;
            workspace.selected_creature = Some(4);
            workspace.debug_scope = DebugScope::Selected;
        }
        app.update();
        let hint = only::<ModeHint>(&mut app);
        assert_eq!(
            app.world().get::<Text>(hint).unwrap().0,
            "Click a free walkable cell to place the debug player."
        );
        app.world_mut().resource_mut::<ReadProjection>().0["debug"] =
            json!({"profile_error":"No start profile"});
        app.update();
        let error = only::<DebugProfileError>(&mut app);
        assert_eq!(
            app.world().get::<Text>(error).unwrap().0,
            "Player unavailable: No start profile"
        );
        app.world_mut().resource_mut::<ReadProjection>().0["debug"]["profile_error"] =
            serde_json::Value::Null;
        let remove = only::<RemovePlayerButton>(&mut app);
        assert_eq!(
            app.world().get::<Node>(remove).unwrap().display,
            Display::None
        );
        app.world_mut().resource_mut::<ReadProjection>().0["debug_player"] =
            json!({"cell":[12, 7], "pixel":[0, 0]});
        app.update();
        assert_eq!(
            app.world().get::<Text>(hint).unwrap().0,
            "Click a destination to move the debug player."
        );
        assert_eq!(
            app.world().get::<Node>(remove).unwrap().display,
            Display::Flex
        );
        app.world_mut().resource_mut::<ReadProjection>().0["debug_player"] =
            serde_json::Value::Null;
        app.update();
        assert_eq!(
            app.world().resource::<ViewerWorkspace>().selected_creature,
            Some(4)
        );
        assert_eq!(
            app.world().get::<Node>(remove).unwrap().display,
            Display::None
        );
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = None;
        app.update();
        let scope_hint = only::<DebugScopeHint>(&mut app);
        assert_eq!(
            app.world().get::<Text>(scope_hint).unwrap().0,
            "Select a creature for squad details, or choose All."
        );
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .debug_scope = DebugScope::All;
        app.update();
        assert!(app.world().get::<Text>(scope_hint).unwrap().0.is_empty());
    }
    #[test]
    fn keyboard_focus_reveals_bottom_display_control_without_fighting_pointer_scroll() {
        use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
        for (width, height) in [(1024, 720), (960, 640)] {
            let mut app = app(width, height);
            app.world_mut()
                .resource_mut::<ViewerWorkspace>()
                .display_open = true;
            for _ in 0..4 {
                app.update();
            }
            let display = only::<DisplayDetails>(&mut app);
            app.world_mut().get_mut::<Node>(display).unwrap().max_height = px(100);
            for _ in 0..3 {
                app.update();
            }
            let target = app
                .world_mut()
                .query::<(Entity, &ButtonAction)>()
                .iter(app.world())
                .find(|(_, action)| matches!(action, ButtonAction::ScopeAll))
                .unwrap()
                .0;
            let before = bounds(app.world(), target);
            let viewport = bounds(app.world(), display);
            assert!(
                before.max.y > viewport.max.y,
                "scope control must start below the clipped viewport at {width}x{height}: {before:?} {viewport:?}"
            );
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(target, FocusCause::Navigated);
            app.world_mut().resource_mut::<InputFocusVisible>().0 = true;
            for _ in 0..3 {
                app.update();
            }
            let after = bounds(app.world(), target);
            let viewport = bounds(app.world(), display);
            assert!(
                inside(after, viewport),
                "keyboard focus did not reveal scope control: {after:?} {viewport:?}"
            );
            assert!(app.world().get::<ScrollPosition>(display).unwrap().y > 0.0);
            app.world_mut()
                .get_mut::<ScrollPosition>(display)
                .unwrap()
                .y = 0.0;
            app.update();
            assert_eq!(
                app.world().get::<ScrollPosition>(display).unwrap().y,
                0.0,
                "unchanged focus must not undo pointer scrolling"
            );
        }
    }
}
