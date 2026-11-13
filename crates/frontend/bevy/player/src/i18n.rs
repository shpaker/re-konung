//! The interface text of the player in the two languages the settings offer.
//!
//! A typed key per string: the compiler lists what exists, the test walks
//! every key in both languages, and a missing translation does not compile.
//! No framework — the player's whole UI is a few dozen lines of `&'static
//! str`; if it ever grows into paragraphs, the keys move to a real catalog
//! without touching the callers.

use kn_app::{SaveError, SessionError};
use kn_settings::Language;

/// One interface string. The payloads are the typed errors the app state
/// carries; their io detail stays in the log, the player sees the sentence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Text {
    /// The game's own name in its release spelling per language.
    Title,
    NewGame,
    /// The hero selection screen's heading.
    HeroTitle,
    /// A hero button when the game data carries no name for it (no texts, an
    /// unnamed record, no game at all); `n` is the 1-based row, not the
    /// `Game.N` number.
    HeroSlot(usize),
    /// Leaving the hero selection back to the main menu.
    Back,
    /// Starting a session with the hero the screen previews.
    Start,
    /// The hero-select detail panel's section headings.
    HeroAttributes,
    HeroSkills,
    HeroOther,
    /// The detail panel's row labels, the original's menu vocabulary
    /// (base attributes, the seven skills, the remaining record facts).
    LabelAgility,
    LabelCharisma,
    LabelStrength,
    LabelEndurance,
    LabelHealth,
    LabelCarrying,
    LabelMaxParty,
    LabelFencing,
    LabelTrading,
    LabelAccuracy,
    LabelCarpentry,
    LabelMedicine,
    LabelIdentification,
    LabelSmithery,
    LabelLevel,
    LabelExperience,
    LabelGold,
    LabelFreePoints,
    Settings,
    Exit,
    Loading,
    /// Why a session could not be built.
    SessionError(SessionError),
    /// The word "Language" as a field label.
    LanguageLabel,
    /// A language's name in itself — the picker always shows native names,
    /// the same word whatever the interface language is.
    LanguageNative(Language),
    Fullscreen,
    Vsync,
    Save,
    Cancel,
    /// Why saving the settings failed.
    SaveFailed(SaveError),
}

/// The string for `text` in `language`.
pub fn t(language: Language, text: Text) -> &'static str {
    match text {
        Text::Title => match language {
            Language::Russian => "Князь: Легенды Лесной страны",
            Language::English => "Konung: Legends of the North",
        },
        Text::NewGame => match language {
            Language::Russian => "Новая игра",
            Language::English => "New game",
        },
        Text::HeroTitle => match language {
            Language::Russian => "Выбор героя",
            Language::English => "Choose your hero",
        },
        Text::HeroSlot(n) => match language {
            Language::Russian => match n {
                1 => "Герой 1",
                2 => "Герой 2",
                3 => "Герой 3",
                _ => "Герой",
            },
            Language::English => match n {
                1 => "Hero 1",
                2 => "Hero 2",
                3 => "Hero 3",
                _ => "Hero",
            },
        },
        Text::Back => match language {
            Language::Russian => "Назад",
            Language::English => "Back",
        },
        Text::Start => match language {
            Language::Russian => "Начать",
            Language::English => "Start",
        },
        Text::HeroAttributes => match language {
            Language::Russian => "Атрибуты",
            Language::English => "Attributes",
        },
        Text::HeroSkills => match language {
            Language::Russian => "Навыки",
            Language::English => "Skills",
        },
        Text::HeroOther => match language {
            Language::Russian => "Прочее",
            Language::English => "Other",
        },
        Text::LabelAgility => match language {
            Language::Russian => "Ловкость",
            Language::English => "Agility",
        },
        Text::LabelCharisma => match language {
            Language::Russian => "Харизма",
            Language::English => "Charisma",
        },
        Text::LabelStrength => match language {
            Language::Russian => "Сила",
            Language::English => "Strength",
        },
        Text::LabelEndurance => match language {
            Language::Russian => "Выносливость",
            Language::English => "Endurance",
        },
        Text::LabelHealth => match language {
            Language::Russian => "Здоровье",
            Language::English => "Health",
        },
        Text::LabelCarrying => match language {
            Language::Russian => "Носимый вес",
            Language::English => "Carry limit",
        },
        Text::LabelMaxParty => match language {
            Language::Russian => "Макс. отряд",
            Language::English => "Max party",
        },
        Text::LabelFencing => match language {
            Language::Russian => "Фехтование",
            Language::English => "Fencing",
        },
        Text::LabelTrading => match language {
            Language::Russian => "Торговля",
            Language::English => "Trading",
        },
        Text::LabelAccuracy => match language {
            Language::Russian => "Меткость",
            Language::English => "Accuracy",
        },
        Text::LabelCarpentry => match language {
            Language::Russian => "Плотницкое дело",
            Language::English => "Carpentry",
        },
        Text::LabelMedicine => match language {
            Language::Russian => "Медицина",
            Language::English => "Medicine",
        },
        Text::LabelIdentification => match language {
            Language::Russian => "Опознание",
            Language::English => "Identification",
        },
        Text::LabelSmithery => match language {
            Language::Russian => "Кузнечное дело",
            Language::English => "Smithery",
        },
        Text::LabelLevel => match language {
            Language::Russian => "Уровень",
            Language::English => "Level",
        },
        Text::LabelExperience => match language {
            Language::Russian => "Опыт",
            Language::English => "Experience",
        },
        Text::LabelGold => match language {
            Language::Russian => "Золото",
            Language::English => "Gold",
        },
        Text::LabelFreePoints => match language {
            Language::Russian => "Свободные очки",
            Language::English => "Free points",
        },
        Text::Settings => match language {
            Language::Russian => "Настройки",
            Language::English => "Settings",
        },
        Text::Exit => match language {
            Language::Russian => "Выход",
            Language::English => "Exit",
        },
        Text::Loading => match language {
            Language::Russian => "Загрузка…",
            Language::English => "Loading…",
        },
        Text::SessionError(reason) => match (language, reason) {
            (Language::Russian, SessionError::Map) => "Не удалось загрузить карту",
            (Language::Russian, SessionError::Scene) => "Не удалось собрать сцену",
            (Language::Russian, SessionError::Textures) => "Не удалось декодировать текстуры",
            (Language::Russian, SessionError::Simulation) => "Не удалось запустить симуляцию",
            (Language::English, SessionError::Map) => "The map could not be loaded",
            (Language::English, SessionError::Scene) => "The scene could not be built",
            (Language::English, SessionError::Textures) => "The textures could not be decoded",
            (Language::English, SessionError::Simulation) => "The simulation could not start",
        },
        Text::LanguageLabel => match language {
            Language::Russian => "Язык",
            Language::English => "Language",
        },
        // Native names: what the picker shows, whatever is applied.
        Text::LanguageNative(name) => match name {
            Language::Russian => "Русский",
            Language::English => "English",
        },
        Text::Fullscreen => match language {
            Language::Russian => "Полный экран",
            Language::English => "Fullscreen",
        },
        Text::Vsync => match language {
            Language::Russian => "Вертикальная синхронизация",
            Language::English => "Vertical sync",
        },
        Text::Save => match language {
            Language::Russian => "Сохранить",
            Language::English => "Save",
        },
        Text::Cancel => match language {
            Language::Russian => "Отмена",
            Language::English => "Cancel",
        },
        Text::SaveFailed(reason) => match (language, reason) {
            (Language::Russian, SaveError::Write) => "Не удалось записать настройки",
            (Language::Russian, SaveError::Replace) => "Не удалось заменить файл настроек",
            (Language::English, SaveError::Write) => "The settings could not be written",
            (Language::English, SaveError::Replace) => "The settings file could not be replaced",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGES: [Language; 2] = [Language::Russian, Language::English];

    fn every_key() -> Vec<Text> {
        let mut keys = vec![
            Text::Title,
            Text::NewGame,
            Text::HeroTitle,
            Text::Back,
            Text::Start,
            Text::HeroAttributes,
            Text::HeroSkills,
            Text::HeroOther,
            Text::LabelAgility,
            Text::LabelCharisma,
            Text::LabelStrength,
            Text::LabelEndurance,
            Text::LabelHealth,
            Text::LabelCarrying,
            Text::LabelMaxParty,
            Text::LabelFencing,
            Text::LabelTrading,
            Text::LabelAccuracy,
            Text::LabelCarpentry,
            Text::LabelMedicine,
            Text::LabelIdentification,
            Text::LabelSmithery,
            Text::LabelLevel,
            Text::LabelExperience,
            Text::LabelGold,
            Text::LabelFreePoints,
            Text::Settings,
            Text::Exit,
            Text::Loading,
            Text::LanguageLabel,
            Text::LanguageNative(Language::Russian),
            Text::LanguageNative(Language::English),
            Text::Fullscreen,
            Text::Vsync,
            Text::Save,
            Text::Cancel,
        ];
        keys.extend([1, 2, 3, 4].map(Text::HeroSlot));
        keys.extend(
            [
                SessionError::Map,
                SessionError::Scene,
                SessionError::Textures,
                SessionError::Simulation,
            ]
            .map(Text::SessionError),
        );
        keys.extend([SaveError::Write, SaveError::Replace].map(Text::SaveFailed));
        keys
    }

    #[test]
    fn every_key_has_a_non_empty_string_in_both_languages() {
        for key in every_key() {
            for language in LANGUAGES {
                assert!(
                    !t(language, key).is_empty(),
                    "{key:?} is empty in {language:?}"
                );
            }
        }
    }

    #[test]
    fn the_two_languages_actually_differ() {
        for key in [
            Text::NewGame,
            Text::Start,
            Text::Settings,
            Text::Exit,
            Text::Fullscreen,
            Text::Vsync,
            Text::Save,
            Text::Cancel,
        ] {
            assert_ne!(
                t(Language::Russian, key),
                t(Language::English, key),
                "{key:?} must be translated"
            );
        }
    }
}
