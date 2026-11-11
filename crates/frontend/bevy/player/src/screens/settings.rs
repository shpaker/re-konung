//! The settings screen: a draft the player edits field by field, applied and
//! persisted by `Save`, discarded by `Cancel`. The screen previews the draft
//! language live; the rest of the interface stays in the applied one until a
//! save lands. The draft itself lives in `kn-app` state — this module only
//! renders it and mirrors it back onto the widgets.

use bevy::feathers::controls::ButtonVariant;
use bevy::feathers::display::caption;
use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene};
use bevy::ui::Checked;
use bevy::ui_widgets::{Activate, ValueChange};
use kn_app::{Action, SaveStatus, SettingsAction, SettingsField};
use kn_settings::{Language, Settings};
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

/// The native-name button of the Russian language.
#[derive(Component, Clone, Default)]
pub(crate) struct LanguageRussian;
/// The native-name button of the English language.
#[derive(Component, Clone, Default)]
pub(crate) struct LanguageEnglish;
/// The fullscreen checkbox.
#[derive(Component, Clone, Default)]
pub(crate) struct FullscreenCheck;
/// The vsync checkbox.
#[derive(Component, Clone, Default)]
pub(crate) struct VsyncCheck;
/// The one-line save status, empty while idle.
#[derive(Component, Clone, Default)]
pub(crate) struct SaveStatusLine;

/// The draft language the current view was built in; a change means the view
/// is stale — its captions are baked at spawn, so it is respawned wholesale.
#[derive(Resource)]
struct BuiltLanguage(kn_settings::Language);

pub(crate) fn spawn(world: &mut World) {
    let fonts = screens::fonts(world);
    let Some(screen) = world.resource::<AppFlow>().state().settings_screen.clone() else {
        return;
    };
    world
        .spawn_scene(root(&fonts, screen.draft))
        .expect("the settings root spawns");
    world.insert_resource(BuiltLanguage(screen.draft.language));
}

fn root(fonts: &KonungFonts, draft: Settings) -> impl Scene {
    let language = draft.language;
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: spacing::SPACE_24,
        }
        BackgroundColor(palette::WINDOW_BG)
        ScreenRoot
        Children [
            {EntityScene(screens::text(
                fonts.bold.clone(),
                typo::TITLE,
                t(language, Text::Settings),
            ))}
            --
            {EntityScene(widgets::panel(
                fonts,
                bsn_list! {
                    @caption(t(language, Text::LanguageLabel))
                    --
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: spacing::SPACE_8,
                    }
                    Children [
                        @widgets::menu_button(
                            fonts,
                            t(language, Text::LanguageNative(Language::Russian)),
                        )
                        LanguageRussian
                        on(click_russian)
                        --
                        @widgets::menu_button(
                            fonts,
                            t(language, Text::LanguageNative(Language::English)),
                        )
                        LanguageEnglish
                        on(click_english)
                    ]
                    --
                    @widgets::checkbox_row(fonts, t(language, Text::Fullscreen))
                    FullscreenCheck
                    on(toggle_fullscreen)
                    --
                    @widgets::checkbox_row(fonts, t(language, Text::Vsync))
                    VsyncCheck
                    on(toggle_vsync)
                    --
                    @caption("")
                    SaveStatusLine
                    --
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: spacing::SPACE_8,
                    }
                    Children [
                        @widgets::menu_button(fonts, t(language, Text::Save))
                        ButtonVariant::Primary
                        on(click_save)
                        --
                        @widgets::menu_button(fonts, t(language, Text::Cancel))
                        on(click_cancel)
                    ]
                },
            ))}
        ]
    }
}

/// Mirrors the settings draft back onto the widgets: the checked boxes, the
/// active language button, the save-failure line. The draft is the only
/// truth; widgets echo it and never the other way (the viewer's
/// `sync_playboxes` pattern). The captions are baked at spawn, so a language
/// change respawns the whole view — that is the live preview.
pub(crate) fn sync(world: &mut World) {
    let Some(screen) = world.resource::<AppFlow>().state().settings_screen.clone() else {
        return;
    };
    let draft = screen.draft;

    let built = world.get_resource::<BuiltLanguage>().map(|built| built.0);
    if built != Some(draft.language) {
        despawn_root(world);
        spawn(world);
    }

    sync_language_button::<LanguageRussian>(world, draft.language == Language::Russian);
    sync_language_button::<LanguageEnglish>(world, draft.language == Language::English);
    sync_checkbox::<FullscreenCheck>(world, draft.video.fullscreen);
    sync_checkbox::<VsyncCheck>(world, draft.video.vsync);

    let string = match screen.status {
        SaveStatus::Idle => "",
        SaveStatus::Failed(reason) => t(draft.language, Text::SaveFailed(reason)),
    };
    // The widget `Text`, qualified: this module's `Text` is the i18n key.
    let mut status = world.query_filtered::<&mut bevy::ui::widget::Text, With<SaveStatusLine>>();
    if let Ok(mut text) = status.single_mut(world)
        && text.0 != string
    {
        text.0 = string.into();
    }
}

/// Kills the current screen tree; the flow's `ScreenRoot` marker identifies it.
fn despawn_root(world: &mut World) {
    let mut roots = world.query_filtered::<Entity, With<crate::flow::ScreenRoot>>();
    let old: Vec<Entity> = roots.iter(world).collect();
    for root in old {
        world.despawn(root);
    }
}

/// One native-name button, primary exactly while its language is the draft's.
fn sync_language_button<Marker: Component>(world: &mut World, active: bool) {
    let mut buttons = world.query_filtered::<&mut ButtonVariant, With<Marker>>();
    if let Ok(mut variant) = buttons.single_mut(world) {
        *variant = if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
    }
}

/// One checkbox, `Checked` exactly while the draft's field is on. Written
/// here and in the click handler both — Bevy's `Checkbox` never writes
/// `Checked` itself (`kn_ui::sync_checked` in world form).
fn sync_checkbox<Marker: Component>(world: &mut World, checked: bool) {
    let mut boxes = world.query_filtered::<Entity, With<Marker>>();
    if let Ok(entity) = boxes.single(world) {
        let mut widget = world.entity_mut(entity);
        if checked {
            widget.insert(Checked);
        } else {
            widget.remove::<Checked>();
        }
    }
}

fn click_russian(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::Settings(SettingsAction::Change(
        SettingsField::Language(Language::Russian),
    )));
}

fn click_english(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::Settings(SettingsAction::Change(
        SettingsField::Language(Language::English),
    )));
}

/// A checkbox reports its flip; the state decides, the widget is mirrored
/// back (Bevy's `Checkbox` never writes `Checked` itself — `sync_checked`).
fn toggle_fullscreen(
    change: On<ValueChange<bool>>,
    mut commands: Commands,
    mut flow: ResMut<AppFlow>,
) {
    kn_ui::sync_checked(&mut commands, change.source, change.value);
    flow.submit(Action::Settings(SettingsAction::Change(
        SettingsField::Fullscreen(change.value),
    )));
}

fn toggle_vsync(change: On<ValueChange<bool>>, mut commands: Commands, mut flow: ResMut<AppFlow>) {
    kn_ui::sync_checked(&mut commands, change.source, change.value);
    flow.submit(Action::Settings(SettingsAction::Change(
        SettingsField::Vsync(change.value),
    )));
}

fn click_save(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::Settings(SettingsAction::Save));
}

fn click_cancel(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::Settings(SettingsAction::Cancel));
}
