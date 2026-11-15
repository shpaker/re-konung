//! Missing originals skip data tests; an existing original/k1 with no discoverable edition is an error.
//! See docs/research/k1/editions.md.

#![allow(dead_code)] // каждый тестовый бинарь использует не всё

use std::path::PathBuf;

use k1_data::repo::GameDir;

pub fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("пропуск: нет original/k1/{edition}/game (docs/research/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} существует, но ни одного издания с папкой game/ в нём нет: \
         тесты на данных не выполнились бы. Как распаковать — docs/research/k1/editions.md",
        root.display()
    );
    dirs
}
