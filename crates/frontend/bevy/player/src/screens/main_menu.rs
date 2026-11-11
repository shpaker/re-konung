//! The title menu: new game, settings, exit — and the reason line of the
//! session that failed, until the next attempt clears it.

use bevy::feathers::display::caption;
use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene};
use bevy::ui_widgets::Activate;
use kn_app::{Action, MainMenuAction, SessionError};
use kn_settings::Language;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, AppliedSettings, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

/// Spawns the menu root for the current state. Called by the flow when the
/// screen becomes `MainMenu`; the whole tree dies with its [`ScreenRoot`].
pub(crate) fn spawn(world: &mut World) {
    let fonts = screens::fonts(world);
    let language = world.resource::<AppliedSettings>().0.language;
    let session_error = world.resource::<AppFlow>().state().session_error;
    world
        .spawn_scene(root(&fonts, language, session_error))
        .expect("the menu root spawns");
}

fn root(
    fonts: &KonungFonts,
    language: Language,
    session_error: Option<SessionError>,
) -> impl Scene {
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
                typo::DISPLAY,
                t(language, Text::Title),
            ))}
            --
            {EntityScene(widgets::panel(
                fonts,
                bsn_list! {
                    @widgets::menu_button(fonts, t(language, Text::NewGame))
                    on(click_new_game)
                    --
                    @widgets::menu_button(fonts, t(language, Text::Settings))
                    on(click_open_settings)
                    --
                    @widgets::menu_button(fonts, t(language, Text::Exit))
                    on(click_exit)
                },
            ))}
            --
            {EntityScene(error_line(language, session_error))}
        ]
    }
}

/// The failure line of the last session; an empty string when there is
/// nothing to say — no node juggling, the layout keeps its place either way.
fn error_line(language: Language, session_error: Option<SessionError>) -> impl Scene {
    let string = match session_error {
        Some(reason) => t(language, Text::SessionError(reason)),
        None => "",
    };
    bsn! {
        Node
        Children [
            @caption(string) TextColor(palette::DANGER)
        ]
    }
}

fn click_new_game(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::MainMenu(MainMenuAction::NewGame));
}

fn click_open_settings(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::MainMenu(MainMenuAction::OpenSettings));
}

fn click_exit(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::MainMenu(MainMenuAction::Exit));
}
