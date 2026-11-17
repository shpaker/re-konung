//! Hero selection, constructor profiles and composed previews.

use k1_data::{
    game_state::{self, GameState},
    repo::GAME_FILES,
};

use crate::{Game, GameError, MapSource, Result, newgame_allocation};

/// The hero each `Game.N` stands for, by the executable's name-table index
/// the original assigns in code (`game_newgame.cpp:305-351`): hero 0 → entry
/// 10, hero 1 → 124, hero 2 → 151 (docs/research/k1/formats/game-state.md). The
/// player-group records of the starting states carry one shared name and are
/// not the heroes' identity.
const HERO_NAME_IDS: [u8; 3] = [10, 124, 151];

/// The description each hero gets, by the executable's new-game line index
/// (`Texts::newgame`): the lines run in the screen-box order — Драгомир
/// (hero 1), Михаил (hero 2), Волк (hero 0) — so hero 0 reads line 2,
/// hero 1 line 0, hero 2 line 1.
const HERO_DESCRIPTION_INDEXES: [usize; 3] = [2, 0, 1];

/// The four occupations the new-game screen offers, with the numbers the
/// original assigns on a click (`game_newgame.cpp:367-413`): the occupation
/// id (the character record's class, `CLASS_*`, game.h:298-301) and the base
/// attributes; every occupation also zeroes the free points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewGameClass {
    /// The character record's class byte (`CLASS_KUPETC=2`, `CLASS_OHOTNIK=3`,
    /// `CLASS_VOJD=4`, `CLASS_VOIN=5`).
    pub class_id: u8,
    pub agility: i16,
    pub charisma: i16,
    pub strength: i16,
    pub endurance: i16,
    /// Carrying capacity, the original's formula over the occupation's
    /// endurance (`game_newgame.cpp:152-154`).
    pub carrying: f32,
    /// The largest party the occupation can lead: the charisma thresholds
    /// (`game_state.cpp:173-190`) over the occupation's charisma.
    pub max_party: usize,
}

/// The four occupations in the original's screen order (Воин is the default
/// the menu opens with, `game.cpp:2323`).
pub fn newgame_classes() -> [NewGameClass; 4] {
    const CLASS: fn(u8, i16, i16, i16, i16) -> NewGameClass =
        |class_id, agility, charisma, strength, endurance| NewGameClass {
            class_id,
            agility,
            charisma,
            strength,
            endurance,
            carrying: ((i32::from(endurance) * 1000) / 3 + 10_000) as f32 * 0.001,
            max_party: max_party_size(class_id, charisma),
        };
    let classes = [CLASS_VOIN, CLASS_OHOTNIK, CLASS_KUPETC, CLASS_VOJD];
    std::array::from_fn(|row| {
        let [agility, charisma, strength, endurance] =
            k1_game::rules::new_game::DEFAULT_ATTRIBUTES[row];
        CLASS(classes[row], agility, charisma, strength, endurance)
    })
}

/// The displayed attributes and derived limits of a validated allocation.
pub fn newgame_profile(class: usize, attributes: Option<[i16; 4]>) -> Option<NewGameClass> {
    let allocation = newgame_allocation(class, attributes)?;
    let mut profile = *newgame_classes().get(class)?;
    [
        profile.agility,
        profile.charisma,
        profile.strength,
        profile.endurance,
    ] = allocation.values;
    profile.carrying = ((i32::from(profile.endurance) * 1000) / 3 + 10_000) as f32 * 0.001;
    profile.max_party = max_party_size(profile.class_id, profile.charisma);
    Some(profile)
}

/// The occupation id constants of the character record (Slavik game.h:298-301).
pub const CLASS_KUPETC: u8 = 2;
pub const CLASS_OHOTNIK: u8 = 3;
pub const CLASS_VOJD: u8 = 4;
pub const CLASS_VOIN: u8 = 5;

/// The two combat specializations of the new-game screen (click boxes 15 and
/// 16, `game_newgame.cpp:420-438`): the fencing/accuracy trade-off plus the
/// craft skills it drags along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewGameCombatStyle {
    pub fencing: i16,
    pub accuracy: i16,
    pub carpentry: i16,
    pub smithery: i16,
}

/// Fencing first (box 15), the menu's default.
pub const COMBAT_FENCING: NewGameCombatStyle = NewGameCombatStyle {
    fencing: 100,
    accuracy: 0,
    carpentry: 1,
    smithery: 10,
};

/// Accuracy first (box 16).
pub const COMBAT_ACCURACY: NewGameCombatStyle = NewGameCombatStyle {
    fencing: 0,
    accuracy: 100,
    carpentry: 2,
    smithery: 5,
};

/// The three secondary-skill sets of the new-game screen (click boxes 17-19,
/// `game_newgame.cpp:440-462`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewGameSecondarySet {
    pub trading: i16,
    pub medicine: i16,
    pub identification: i16,
}

/// The default set (box 17).
pub const SECONDARY_DEFAULT: NewGameSecondarySet = NewGameSecondarySet {
    trading: 10,
    medicine: 50,
    identification: 20,
};

/// The trader-leaning set (box 18).
pub const SECONDARY_TRADING: NewGameSecondarySet = NewGameSecondarySet {
    trading: 20,
    medicine: 50,
    identification: 10,
};

/// The healer-leaning set (box 19).
pub const SECONDARY_MEDICINE: NewGameSecondarySet = NewGameSecondarySet {
    trading: 10,
    medicine: 100,
    identification: 10,
};

/// The composed model preview of a hero: straight-alpha RGBA over the
/// layers' bounding box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroPreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One hero of the new-game flow: what depends on the hero (the name, the
/// description, the portraits) plus the record facts of the starting state
/// (level, experience, gold, free points, health — offsets 0x33–0x5C,
/// confirmed for both editions). The menu's attribute and skill numbers do
/// NOT depend on the hero: they come from the occupation and the
/// specialization choices — [`newgame_classes`], [`COMBAT_FENCING`] and
/// friends.
#[derive(Debug, Clone, PartialEq)]
pub struct HeroSummary {
    /// The `Game.N` starting-state number.
    pub hero: u32,
    /// The hero's name from the executable's name tables; empty when the
    /// tables are missing or the number is not one of the three heroes.
    pub name: String,
    /// The hero's description from the executable's new-game lines; empty
    /// under the same conditions.
    pub description: String,
    /// The `NEWH*.RES` record of the hero's plain portrait
    /// (`newh_res::PORTRAIT_RECORDS`); the selected variant is +1.
    pub portrait: u8,
    /// The `NEWH*.RES` record of the hero's selected portrait.
    pub portrait_selected: u8,
    pub level: u8,
    pub experience: i32,
    pub gold: i32,
    pub free_points: i32,
    /// Health on the interface scale: the file holds 0…1600 and the original
    /// interface prints `(HP + 15) / 16` points (`game_state.cpp:47`).
    pub health: i16,
}

/// The original's max party size (`game_state.cpp:173-190`): the number of
/// charisma thresholds the current charisma meets, plus one.
fn max_party_size(class_id: u8, charisma: i16) -> usize {
    const PARTY_SIZE_CHARM: [[i16; 10]; 5] = [
        [15, 21, 28, 36, 45, 55, 65, 74, 83, 95],
        [15, 21, 28, 36, 45, 55, 65, 74, 83, 95],
        [17, 23, 30, 38, 47, 57, 67, 76, 85, 97],
        [15, 21, 28, 36, 45, 55, 65, 74, 83, 95],
        [20, 28, 36, 45, 55, 65, 74, 83, 95, 110],
    ];
    // The row indexes the occupation class (1..=5); the original reads past
    // the table for anything else, which no shipped record carries.
    let Some(row) = usize::from(class_id & 7)
        .checked_sub(1)
        .and_then(|class| PARTY_SIZE_CHARM.get(class))
    else {
        return 0;
    };
    let passed = row
        .iter()
        .take_while(|&&threshold| charisma >= threshold)
        .count();
    (passed + 1).min(10)
}

impl Game {
    /// Applies the constructor to an in-memory starting state. Disk files stay intact.
    pub fn configure_new_game(
        &mut self,
        hero: u32,
        class: usize,
        combat: usize,
        secondary: usize,
        attributes: Option<[i16; 4]>,
    ) -> Result<()> {
        let invalid = || GameError::Io(std::io::Error::other("invalid new-game configuration"));
        let occupation = *newgame_classes().get(class).ok_or_else(invalid)?;
        if hero > 2 || combat > 1 || secondary > 2 {
            return Err(invalid());
        }
        let allocation = newgame_allocation(class, attributes).ok_or_else(invalid)?;
        let mut state = game_state::parse(&self.dir.state_bytes(hero, GAME_FILES)?)?;
        let slot = game_state::on_map(
            &state,
            game_state::start_map(hero as usize),
            Some(hero as usize),
        )
        .start
        .and_then(|group| group.members().next())
        .ok_or_else(invalid)?;
        let character = state.characters[slot].as_mut().ok_or_else(invalid)?;
        let style = if combat == 0 {
            COMBAT_FENCING
        } else {
            COMBAT_ACCURACY
        };
        let skills = [SECONDARY_DEFAULT, SECONDARY_TRADING, SECONDARY_MEDICINE][secondary];
        character.class_id = occupation.class_id;
        character.level = 1;
        character.free_points = allocation.free_points;
        character.direction = 5;
        character.name_id = [10, 124, 151][hero as usize];
        character.sprite_set = hero as u8;
        character.palette_offset = [74, 33, 39][hero as usize] * 512;
        character.attributes[0] = allocation.values[0];
        character.attributes[1] = allocation.values[0];
        character.attributes[2] = allocation.values[1];
        character.attributes[3] = allocation.values[1];
        character.attributes[5] = allocation.values[2];
        character.attributes[6] = allocation.values[2];
        character.attributes[7] = allocation.values[3];
        character.attributes[8] = allocation.values[3];
        character.skills[..7].copy_from_slice(&[
            style.fencing,
            skills.trading,
            style.accuracy,
            style.carpentry,
            skills.medicine,
            skills.identification,
            style.smithery,
        ]);
        self.session_state = Some((hero, state));
        Ok(())
    }

    /// Номера `Game.N` (или `TGame.N`) — начальных состояний новой игры.
    pub fn heroes(&self, source: MapSource) -> Result<Vec<u32>> {
        Ok(self.dir.state_numbers(source.file_set())?)
    }

    /// The name of each hero, the way the original's new-game menu shows it:
    /// the first record of the player group, assembled by the game's own rule
    /// from the executable's name tables (docs/research/k1/formats/game-state.md,
    /// bytes 0xD4/0xD5; `game_newgame.cpp:306-351`). Game data, not interface
    /// strings — an edition without the tables, or a record without a name,
    /// yields an empty string the caller renders with its own fallback.
    pub fn hero_names(&self, source: MapSource) -> Result<Vec<(u32, String)>> {
        Ok(self
            .hero_summaries(source)?
            .into_iter()
            .map(|summary| (summary.hero, summary.name))
            .collect())
    }

    /// Each hero the way the original's new-game menu presented them
    /// (`game_newgame.cpp:115-186`). The name and the description come from
    /// the executable's tables by the hero's number — the original assigns
    /// the name in code (`game_newgame.cpp:305-351`), because the
    /// player-group record the starting states carry is the same in every
    /// `Game.N` of both editions (measured) and is not the hero's identity.
    /// Level, experience, gold, free points and health are our additions
    /// from the record (the menu never printed them). An unknown hero
    /// number, or texts the exe does not carry, yield empty strings the
    /// caller renders with its own fallback.
    pub fn hero_summaries(&self, source: MapSource) -> Result<Vec<HeroSummary>> {
        let files = source.file_set();
        let texts = self.resources.tables.texts.as_ref();
        let mut summaries = Vec::new();
        for hero in self.heroes(source)? {
            let state_bytes = self.dir.state_bytes(hero, files)?;
            let state = game_state::parse(&state_bytes)?;
            let entities = game_state::on_map(
                &state,
                game_state::start_map(hero as usize),
                Some(hero as usize),
            );
            let empty: [String; 0] = [];
            let names: &[String] = texts.map_or(&empty[..], |texts| &texts.names);
            let newgame: &[String] = texts.map_or(&empty[..], |texts| &texts.newgame);
            let by_name_table = |table: &[String], index: usize| -> String {
                table
                    .get(index)
                    .map(|text| text.trim().to_string())
                    .unwrap_or_default()
            };
            let name = HERO_NAME_IDS
                .get(hero as usize)
                .map(|&name_id| by_name_table(names, usize::from(name_id)))
                .unwrap_or_default();
            let description = HERO_DESCRIPTION_INDEXES
                .get(hero as usize)
                .map(|&index| by_name_table(newgame, index))
                .unwrap_or_default();
            let (portrait, portrait_selected) =
                match k1_data::newh_res::PORTRAIT_RECORDS.get(hero as usize) {
                    Some(&selected) => ((selected + 1) as u8, selected as u8),
                    None => (0, 0),
                };
            let record = entities
                .start
                .and_then(|group| group.members().next())
                .and_then(|member| state.characters.get(member).and_then(Option::as_ref));
            let Some(character) = record else {
                summaries.push(HeroSummary {
                    hero,
                    name,
                    description,
                    portrait,
                    portrait_selected,
                    level: 0,
                    experience: 0,
                    gold: 0,
                    free_points: 0,
                    health: 0,
                });
                continue;
            };
            let health = (i32::from(character.attributes[4]) + 15) / 16;
            summaries.push(HeroSummary {
                hero,
                name,
                description,
                portrait,
                portrait_selected,
                level: character.level,
                experience: character.experience,
                gold: character.gold,
                free_points: character.free_points,
                health: health as i16,
            });
        }
        Ok(summaries)
    }

    /// The 20 lines of the original's new-game menu from the executable's
    /// tables (docs/research/k1/formats/executable.md): 0–2 the heroes' descriptions
    /// in the screen-box order, 3–6 the occupations' descriptions, 7–19 the
    /// hover hints of the menu's numbers. Trimmed; empty when the edition
    /// carries no texts. Our presentation shows them as tooltips.
    pub fn newgame_texts(&self) -> Vec<String> {
        self.resources
            .tables
            .texts
            .as_ref()
            .map(|texts| {
                texts
                    .newgame
                    .iter()
                    .map(|text| text.trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Actual starting equipment from Game.N, composed with the chosen hero's
    /// original model and palette. Unlike the original menu's demonstration
    /// outfit, this preview agrees with the playable record.
    pub fn hero_preview(&self, hero: u32) -> Result<HeroPreview> {
        if hero >= 3 {
            return Err(GameError::Io(std::io::Error::other("invalid hero")));
        }
        let state = game_state::parse(&self.dir.state_bytes(hero, GAME_FILES)?)?;
        self.preview_state(hero, &state)
    }

    /// Composes the actual configured session record, including its original equipment.
    pub fn configured_hero_preview(&self, hero: u32) -> Result<HeroPreview> {
        let state = self
            .session_state
            .as_ref()
            .filter(|(chosen, _)| *chosen == hero)
            .map(|(_, state)| state)
            .ok_or_else(|| GameError::Io(std::io::Error::other("no configured hero")))?;
        self.preview_state(hero, state)
    }

    fn preview_state(&self, hero: u32, state: &GameState) -> Result<HeroPreview> {
        let slot = game_state::on_map(
            state,
            game_state::start_map(hero as usize),
            Some(hero as usize),
        )
        .start
        .and_then(|group| group.members().next())
        .ok_or_else(|| GameError::Io(std::io::Error::other("no player record")))?;
        let mut character = state
            .characters
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or_else(|| GameError::Io(std::io::Error::other("missing player record")))?
            .clone();
        character.sprite_set = hero as u8;
        character.palette_offset = [74, 33, 39][hero as usize] * 512;
        character.direction = 5;
        character.state = 0;
        character.frame = 0;
        let sprite = self
            .resources
            .with_graphics(|graph, objects| {
                k1_data::characters::compose(
                    &character,
                    0,
                    0,
                    state,
                    graph,
                    objects,
                    &self.resources.tables,
                    None,
                )
            })
            .map_err(|error| GameError::Io(std::io::Error::other(error.to_string())))?
            .map_err(|error| GameError::Io(std::io::Error::other(error.to_string())))?;
        let Some(sprite) = sprite else {
            return Err(GameError::Io(std::io::Error::other(
                "the hero model has no idle frame",
            )));
        };
        // Composite the layers over their bounding box: body first, gear
        // over it, in the compose order.
        let mut origin_x = i32::MAX;
        let mut origin_y = i32::MAX;
        let mut right = i32::MIN;
        let mut bottom = i32::MIN;
        for layer in &sprite.layers {
            origin_x = origin_x.min(layer.x);
            origin_y = origin_y.min(layer.y);
            right = right.max(layer.x + layer.image.width as i32);
            bottom = bottom.max(layer.y + layer.image.height as i32);
        }
        let width = (right - origin_x).max(0) as u32;
        let height = (bottom - origin_y).max(0) as u32;
        let mut rgba = vec![0u8; usize::try_from(width * height * 4).unwrap_or(usize::MAX)];
        for layer in &sprite.layers {
            let Some(key_rgba) = self
                .resources
                .with_graphics(|graph, objects| {
                    k1_game::presentation::textures::decode(
                        &k1_game::presentation::textures::TextureKey::SpriteImage {
                            source: layer.source,
                            image_id: layer.image_id as u16,
                            palette: layer.palette_offset as u32,
                        },
                        graph,
                        objects,
                        &self.resources.lights,
                        &self.resources.flames,
                    )
                })
                .map_err(|error| GameError::Io(std::io::Error::other(error.to_string())))?
                .map_err(|error| GameError::Io(std::io::Error::other(error.to_string())))?
            else {
                continue;
            };
            // Paste with alpha.
            for row in 0..key_rgba.height {
                for column in 0..key_rgba.width {
                    let source = (row * key_rgba.width + column) as usize * 4;
                    let alpha = key_rgba.data.get(source + 3).copied().unwrap_or(0);
                    if alpha == 0 {
                        continue;
                    }
                    let (px, py) = (
                        layer.x - origin_x + column as i32,
                        layer.y - origin_y + row as i32,
                    );
                    if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                        continue;
                    }
                    let target = (py as u32 * width + px as u32) as usize * 4;
                    rgba[target..target + 4].copy_from_slice(&key_rgba.data[source..source + 4]);
                }
            }
        }
        Ok(HeroPreview {
            width,
            height,
            rgba,
        })
    }
}
