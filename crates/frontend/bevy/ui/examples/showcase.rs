//! kn-ui showcase: the design-system reference — tokens, the konung theme, the
//! bundled fonts and every widget in every state, in one scrollable window and
//! without a game scene. This is the visual acceptance surface for kn-ui.
//!
//! What to verify by hand:
//! * Palette and typography (ru + en, PT Sans carries the Cyrillic).
//! * Buttons: hover and pressed recolor live under the pointer; the disabled
//!   one shows the "not allowed" cursor and never activates; the primary
//!   variant rides the accent fill.
//! * Checkboxes: the live one toggles on click AND on Space; the state samples
//!   (unchecked / checked / disabled) are frozen as spawned.
//! * Slider: the bar drags, arrow keys step by 5, Home/End jump to the ends;
//!   the readout and the thumb track the value.
//! * Select: opens a popup, the readout follows the picked option.
//! * Keyboard: Tab / Shift+Tab walk every control on the page with a visible
//!   amber focus ring (the konung `FOCUS_RING` token); Enter and Space
//!   activate the focused control. A click hides the ring (pointer modality),
//!   Tab brings it back.

use bevy::color::Color;
use bevy::feathers::{
    controls::{
        ButtonVariant, FeathersSelect, FeathersSlider, OptionIndex, list_rows_from_strings,
    },
    display::caption,
    font_styles::InheritableFont,
    theme::InheritableThemeTextColor,
    tokens,
};
use bevy::prelude::*;
use bevy::scene::EntityScene;
use bevy::text::{FontSize, FontWeight};
use bevy::ui::widget::Text;
use bevy::ui::{Checked, InteractionDisabled, Overflow};
use bevy::ui_widgets::{
    Activate, ScrollArea, SliderStep, SliderValue, ValueChange, slider_self_update,
};
use kn_ui::{KonungFonts, KonungUiPlugin, palette, spacing, sync_checked, typo, widgets};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "re-konung UI showcase".to_string(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(KonungUiPlugin)
        .insert_resource(PressCount(0))
        .add_observer(count_press)
        .add_observer(toggle_live_checkbox)
        .add_observer(toggle_music)
        .add_observer(readout_slider)
        .add_observer(pick_resolution)
        .add_systems(Startup, (spawn_camera, build_showcase))
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn build_showcase(world: &mut World) {
    let fonts = world.resource::<KonungFonts>().clone();
    world
        .spawn_scene(showcase(&fonts))
        .expect("the showcase scene spawns");
}

fn showcase(fonts: &KonungFonts) -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
        }
        BackgroundColor(palette::WINDOW_BG)
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        InheritableThemeTextColor(tokens::TEXT_MAIN)
        Children [
            // Wheel scrolling is not implied by Overflow::Scroll: the marker
            // plus the UiWidgetsPlugins observer (in DefaultPlugins) drives it
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: spacing::SPACE_24,
                padding: UiRect::axes(spacing::SPACE_24, spacing::SPACE_24),
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            Children [
                {EntityScene(text_block(fonts.bold.clone(), typo::DISPLAY, "re-konung UI showcase"))}
                --
                {EntityScene(hint(fonts, "Дизайн-система kn-ui — токены, тема, шрифты и все состояния контролов. Прокрутка — колесом мыши. The kn-ui design system; scroll with the wheel."))}
                --
                {EntityScene(keyboard_section(fonts))}
                --
                {EntityScene(palette_section())}
                --
                {EntityScene(typography_section(fonts))}
                --
                {EntityScene(buttons_section(fonts))}
                --
                {EntityScene(checkboxes_section(fonts))}
                --
                {EntityScene(slider_section(fonts))}
                --
                {EntityScene(select_section(fonts))}
                --
                {EntityScene(panel_section(fonts))}
            ]
        ]
    }
}

/// A caption sized and faced by an `InheritableFont` on its wrapper node: a
/// `TextFont` written directly on the span would reset the font to Bevy's
/// default, the propagation travels over `ThemedText` spans instead.
fn text_block(font: Handle<Font>, size: FontSize, text: &str) -> impl Scene {
    bsn! {
        Node
        InheritableFont {
            font: {font},
            font_size: {size},
            weight: FontWeight::NORMAL,
        }
        Children [@caption(text)]
    }
}

/// A dim secondary line: the color patch on the text_block root overrides the
/// page-wide `TEXT_MAIN` for this subtree only.
fn hint(fonts: &KonungFonts, text: &str) -> impl Scene {
    bsn! {
        @text_block(fonts.regular.clone(), typo::CAPTION, text)
        InheritableThemeTextColor(tokens::TEXT_DIM)
    }
}

fn section(fonts: &KonungFonts, title: &str, children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: spacing::SPACE_8,
        }
        Children [
            {EntityScene(text_block(fonts.bold.clone(), typo::TITLE, title))}
            --
            {children}
        ]
    }
}

fn keyboard_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Keyboard & mouse — Клавиатура и мышь",
        bsn_list! {
            @caption("Tab / Shift+Tab — фокус по всем контролам страницы, кольцо фокуса янтарное.")
            --
            @caption("Enter и Space нажимают кнопку или флажок; ← → шагают ползунок, Home и End — в края.")
            --
            @caption("Клик прячет кольцо фокуса (мышиный режим), Tab возвращает. Hover и нажатие живые под курсором.")
        },
    )
}

fn palette_section() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_8,
            row_gap: spacing::SPACE_8,
            flex_wrap: FlexWrap::Wrap,
        }
        Children [
            {EntityScene(swatch("window", palette::WINDOW_BG))}
            --
            {EntityScene(swatch("surface", palette::SURFACE))}
            --
            {EntityScene(swatch("raised", palette::SURFACE_RAISED))}
            --
            {EntityScene(swatch("border", palette::BORDER))}
            --
            {EntityScene(swatch("text", palette::TEXT))}
            --
            {EntityScene(swatch("muted", palette::TEXT_MUTED))}
            --
            {EntityScene(swatch("accent", palette::ACCENT))}
            --
            {EntityScene(swatch("accent hover", palette::ACCENT_HOVER))}
            --
            {EntityScene(swatch("danger", palette::DANGER))}
        ]
    }
}

fn swatch(label: &str, color: Color) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: spacing::SPACE_4,
            align_items: AlignItems::Center,
        }
        Children [
            Node {
                width: px(64.0),
                height: px(32.0),
                border: px(1.0),
                border_radius: px(4.0),
            }
            BackgroundColor({color})
            // BorderColor is a four-field struct in Bevy 0.20, not a newtype
            BorderColor {
                top: palette::BORDER,
                right: palette::BORDER,
                bottom: palette::BORDER,
                left: palette::BORDER,
            }
            --
            @caption(label)
        ]
    }
}

fn typography_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Typography",
        bsn_list! {
            {EntityScene(text_block(fonts.regular.clone(), typo::CAPTION, "Caption 12 — Новая игра New game"))}
            --
            {EntityScene(text_block(fonts.regular.clone(), typo::BODY, "Body 14 — Новая игра New game"))}
            --
            {EntityScene(text_block(fonts.regular.clone(), typo::TITLE, "Title 18 — Новая игра New game"))}
            --
            {EntityScene(text_block(fonts.regular.clone(), typo::DISPLAY, "Display 24 — Новая игра New game"))}
            --
            {EntityScene(text_block(fonts.bold.clone(), typo::BODY, "Bold 14 — Настройки Settings"))}
        },
    )
}

fn buttons_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Buttons — Кнопки",
        bsn_list! {
            {EntityScene(hint(fonts, "Обычная, основная и неактивная. Hover и нажатие живые — наведите и удерживайте. Normal, primary (accent fill), disabled; hover and pressed are live under the pointer."))}
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_8,
                row_gap: spacing::SPACE_8,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
            }
            Children [
                @widgets::menu_button(fonts, "Обычная — Normal")
                --
                // Primary is a Feathers variant, not a hand-patched fill: the
                // konung theme maps its tokens onto the accent palette
                @widgets::menu_button(fonts, "Основная — Primary")
                ButtonVariant::Primary
                --
                @widgets::menu_button(fonts, "Неактивная — Disabled")
                InteractionDisabled
            ]
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_8,
                align_items: AlignItems::Center,
            }
            Children [
                @widgets::menu_button(fonts, "Нажми меня — Press me")
                CountButton
                on(count_press)
                --
                @caption("Нажатий — presses: 0") PressReadout
            ]
        },
    )
}

fn checkboxes_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Checkboxes — Флажки",
        bsn_list! {
            {EntityScene(hint(fonts, "Первая строка — застывшие состояния. Живой флажок переключается мышью и Space, состояние читается справа. The first row holds frozen states; the live one toggles on click and Space."))}
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_16,
                row_gap: spacing::SPACE_8,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
            }
            Children [
                @widgets::checkbox_row(fonts, "Снят — Unchecked")
                --
                @widgets::checkbox_row(fonts, "Отмечен — Checked")
                Checked
                --
                @widgets::checkbox_row(fonts, "Отмечен, неактивен — Checked, disabled")
                Checked
                InteractionDisabled
                --
                @widgets::checkbox_row(fonts, "Неактивен — Disabled")
                InteractionDisabled
            ]
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_16,
                align_items: AlignItems::Center,
            }
            Children [
                @widgets::checkbox_row(fonts, "Живой — Live")
                LiveCheckbox
                on(toggle_live_checkbox)
                --
                @caption("выключено — off") CheckboxReadout
            ]
        },
    )
}

fn slider_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Slider — Ползунок",
        bsn_list! {
            {EntityScene(hint(fonts, "Тяните бар мышью; стрелки шагают по 5 %, Home и End — в края. Drag the bar; arrows step by 5, Home/End jump."))}
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_32,
                row_gap: spacing::SPACE_8,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::FlexStart,
            }
            Children [
                Node {
                    width: px(300.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_4,
                }
                InheritableThemeTextColor(tokens::TEXT_DIM)
                Children [
                    @caption("Живой — Live")
                    --
                    // Built inline, not through widgets::slider_row: the step,
                    // the marker and the observers belong to the slider
                    // entity, and the helper wraps it in a caption column
                    @FeathersSlider { @min: 0.0, @max: 100.0 }
                    SliderValue(40.0)
                    SliderStep(5.0)
                    LiveSlider
                    on(slider_self_update)
                    on(readout_slider)
                    --
                    @caption("Громкость — volume: 40 %") SliderReadout
                ]
                --
                Node {
                    width: px(300.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_4,
                }
                InheritableThemeTextColor(tokens::TEXT_DIM)
                Children [
                    @caption("Неактивный — Disabled")
                    --
                    @FeathersSlider { @min: 0.0, @max: 100.0 }
                    SliderValue(70.0)
                    InteractionDisabled
                ]
            ]
        },
    )
}

/// Options of the select: period-appropriate resolutions.
const RESOLUTIONS: [&str; 4] = ["640×480", "800×600", "1024×768", "1280×800"];

fn select_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Select — Выпадающий список",
        bsn_list! {
            {EntityScene(hint(fonts, "Кликните, чтобы открыть; выбор отражается в строке ниже. Click to open; the readout follows the picked option."))}
            --
            @FeathersSelect {
                @options: list_rows_from_strings(RESOLUTIONS, Some(3)),
                @max_visible: 4,
            }
            Node { width: px(220.0) }
            --
            @caption("Выбрано — chosen: 1280×800") SelectReadout
        },
    )
}

fn panel_section(fonts: &KonungFonts) -> impl Scene {
    section(
        fonts,
        "Panel — Настройки, композиция — settings preview",
        EntityScene(widgets::panel(
            fonts,
            bsn_list! {
                {EntityScene(text_block(fonts.bold.clone(), typo::TITLE, "Настройки — Settings"))}
                --
                @widgets::checkbox_row(fonts, "Музыка — Music")
                Checked
                MusicToggle
                on(toggle_music)
                --
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_4,
                }
                InheritableThemeTextColor(tokens::TEXT_DIM)
                Children [
                    @caption("Громкость — Volume")
                    --
                    @FeathersSlider { @min: 0.0, @max: 100.0 }
                    SliderValue(40.0)
                    SliderStep(1.0)
                    on(slider_self_update)
                ]
                --
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: spacing::SPACE_8,
                }
                Children [
                    @widgets::menu_button(fonts, "ОК")
                    ButtonVariant::Primary
                    --
                    @widgets::menu_button(fonts, "Отмена — Cancel")
                ]
            },
        )),
    )
}

/// The live button of the Buttons section.
#[derive(Component, Clone, Copy, Default)]
struct CountButton;

/// Presses of [`CountButton`].
#[derive(Resource, Default)]
struct PressCount(u32);

/// The interactive checkbox of the Checkboxes section.
#[derive(Component, Clone, Copy, Default)]
struct LiveCheckbox;

/// The music toggle of the settings preview panel.
#[derive(Component, Clone, Copy, Default)]
struct MusicToggle;

/// The interactive slider of the Slider section.
#[derive(Component, Clone, Copy, Default)]
struct LiveSlider;

/// Readout captions, each marked so its observer finds its own.
#[derive(Component, Clone, Copy, Default)]
struct PressReadout;
#[derive(Component, Clone, Copy, Default)]
struct CheckboxReadout;
#[derive(Component, Clone, Copy, Default)]
struct SliderReadout;
#[derive(Component, Clone, Copy, Default)]
struct SelectReadout;

/// Counts presses of the live button into its readout.
fn count_press(
    activate: On<Activate>,
    buttons: Query<&CountButton>,
    mut presses: ResMut<PressCount>,
    mut readout: Query<&mut Text, With<PressReadout>>,
) {
    if buttons.get(activate.entity).is_err() {
        return;
    }
    presses.0 += 1;
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("Нажатий — presses: {}", presses.0);
    }
}

/// Mirrors the live checkbox state back onto the widget and its readout: the
/// widget itself never writes `Checked` ([`kn_ui::sync_checked`]).
fn toggle_live_checkbox(
    change: On<ValueChange<bool>>,
    checkboxes: Query<&LiveCheckbox>,
    mut readout: Query<&mut Text, With<CheckboxReadout>>,
    mut commands: Commands,
) {
    if checkboxes.get(change.source).is_err() {
        return;
    }
    sync_checked(&mut commands, change.source, change.value);
    if let Ok(mut text) = readout.single_mut() {
        text.0 = if change.value {
            "включено — on".to_string()
        } else {
            "выключено — off".to_string()
        };
    }
}

/// The panel's music toggle: only mirrors the state, no readout.
fn toggle_music(
    change: On<ValueChange<bool>>,
    toggles: Query<&MusicToggle>,
    mut commands: Commands,
) {
    if toggles.get(change.source).is_ok() {
        sync_checked(&mut commands, change.source, change.value);
    }
}

/// Writes the live slider's value into its readout; the thumb follows via
/// [`slider_self_update`].
fn readout_slider(
    change: On<ValueChange<f32>>,
    sliders: Query<&LiveSlider>,
    mut readout: Query<&mut Text, With<SliderReadout>>,
) {
    if sliders.get(change.source).is_err() {
        return;
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("Громкость — volume: {:.0} %", change.value);
    }
}

/// Reads the picked select option by its [`OptionIndex`] into the readout.
fn pick_resolution(
    change: On<ValueChange<Entity>>,
    options: Query<&OptionIndex>,
    mut readout: Query<&mut Text, With<SelectReadout>>,
) {
    let Ok(option) = options.get(change.value) else {
        return;
    };
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("Выбрано — chosen: {}", RESOLUTIONS[option.0]);
    }
}
