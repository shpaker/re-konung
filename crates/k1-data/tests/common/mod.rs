//! Общий хелпер тестов на данных игры из `original/` (в git их нет).
//!
//! Без распакованного издания тест пропускается: пустой список папок,
//! аналог pytest.skip в tests/conftest.py. Как получить original/ —
//! docs/k1/editions.md.

#![allow(dead_code)] // каждый тестовый бинарь использует не всё

use std::path::PathBuf;

use k1_data::repo::GameDir;

pub fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("пропуск: нет original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect()
}
