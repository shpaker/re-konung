//! The same four-attribute presentation for allocation and in-game inspection.
//! Values arrive already computed; this widget owns no creation draft or game rules.

use bevy::feathers::{display::caption, font_styles::InheritableFont};
use bevy::prelude::*;
use bevy::scene::Scene;
use bevy::text::FontWeight;
use kn_settings::Language;
use kn_ui::{KonungFonts, palette, spacing, typo};

use crate::i18n::{Text, t};

/// Read-only mode creates no editing controls or activation observers.
pub enum AttributeMode {
    ReadOnly,
    Editable {
        baseline: [i16; 4],
        actions: [Box<dyn Scene>; 4],
    },
}

type AttributeEdit = (i16, Box<dyn Scene>);

/// Values are ordered agility, charisma, strength, endurance, as in the facade.
/// The caller supplies action scenes only while editing; inspection needs values alone.
pub fn attributes(
    fonts: &KonungFonts,
    language: Language,
    values: [i16; 4],
    mode: AttributeMode,
) -> impl Scene {
    let edits: [Option<AttributeEdit>; 4] = match mode {
        AttributeMode::ReadOnly => std::array::from_fn(|_| None),
        AttributeMode::Editable { baseline, actions } => {
            let mut actions = actions.into_iter();
            std::array::from_fn(|index| Some((baseline[index], actions.next().unwrap())))
        }
    };
    let labels = [
        Text::LabelAgility,
        Text::LabelCharisma,
        Text::LabelStrength,
        Text::LabelEndurance,
    ];
    let rows = values
        .into_iter()
        .zip(labels)
        .zip(edits)
        .map(|((value, label), edit)| {
            attribute_row(fonts, language, t(language, label), value, edit)
        })
        .collect::<Vec<_>>();
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: spacing::SPACE_4 }
        Children [{rows}]
    }
}

fn attribute_row(
    fonts: &KonungFonts,
    language: Language,
    label: &str,
    value: i16,
    edit: Option<AttributeEdit>,
) -> impl Scene {
    let changed = edit.as_ref().is_some_and(|(initial, _)| value != *initial);
    let background = if changed {
        palette::SURFACE_RAISED
    } else {
        palette::SURFACE
    };
    let value_color = if changed {
        palette::ACCENT
    } else {
        palette::TEXT
    };
    let value_label = value.to_string();
    let baseline = edit
        .as_ref()
        .filter(|_| changed)
        .map(|(initial, _)| format!("{}: {initial}", t(language, Text::Baseline)))
        .unwrap_or_default();
    let label_width = if edit.is_some() {
        px(130)
    } else {
        percent(100)
    };
    let row_height = if edit.is_some() { px(44) } else { px(28) };
    let action_scenes = edit
        .map(|(_, actions)| actions)
        .into_iter()
        .collect::<Vec<_>>();
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: spacing::SPACE_8, padding: UiRect::all(px(4)), border_radius: px(4), min_height: row_height, flex_shrink: 0.0 }
        BackgroundColor(background)
        Children [
            Node { flex_direction: FlexDirection::Column, width: label_width, flex_shrink: 0.0 }
            InheritableFont { font: {fonts.regular.clone()}, font_size: typo::BODY, weight: FontWeight::NORMAL }
            Children [
                Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, column_gap: spacing::SPACE_8 }
                InheritableFont { font: {fonts.regular.clone()}, font_size: typo::BODY, weight: FontWeight::NORMAL }
                Children [
                    @caption(label) TextColor(palette::TEXT_MUTED)
                    --
                    @caption(value_label.as_str()) TextColor(value_color)
                ]
                --
                Node
                InheritableFont { font: {fonts.regular.clone()}, font_size: typo::CAPTION, weight: FontWeight::NORMAL }
                Children [@caption(baseline.as_str()) TextColor(palette::TEXT_MUTED)]
            ]
            --
            {action_scenes}
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_renders_runtime_values_without_allocation_controls() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ))
        .init_asset::<Font>()
        .insert_resource(kn_ui::konung_theme());
        let fonts = kn_ui::konung_fonts(&mut app.world_mut().resource_mut::<Assets<Font>>());
        // Runtime inspection must accept values beyond creation-time budgets and limits.
        app.world_mut()
            .spawn_scene(attributes(
                &fonts,
                Language::Russian,
                [125, 65, 111, 74],
                AttributeMode::ReadOnly,
            ))
            .unwrap();
        app.update();
        let shown = app
            .world_mut()
            .query::<&bevy::ui::widget::Text>()
            .iter(app.world())
            .map(|text| text.0.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "Ловкость",
            "Харизма",
            "Сила",
            "Выносливость",
            "125",
            "65",
            "111",
            "74",
        ] {
            assert!(shown.contains(&expected), "missing runtime stat {expected}");
        }
        assert!(!shown.iter().any(|text| text.contains("Исходное")
            || text.contains("очки")
            || *text == "+"
            || *text == "−"));
        assert_eq!(
            app.world_mut()
                .query::<&bevy::ui_widgets::Button>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
