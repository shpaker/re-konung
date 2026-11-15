//! `NEWH*.RES` on the real files of both editions (absent from git).
//!
//! Without an unpacked edition the tests are skipped (common helper).
//! Confirms the format to the byte: 13 records, each consumed to its
//! declared size, the series ending on the last byte of the file; the
//! portrait records (3–8) are byte-identical between the editions.

mod common;

use common::game_dirs;
use k1_data::newh_res;

const RECORDS: usize = 13;

#[test]
fn every_menu_file_parses_to_the_last_byte() {
    for (edition, game) in game_dirs() {
        for width in ["640", "800"] {
            for language in ["R", "E"] {
                let path = game.root().join(format!("NEWH{width}{language}.RES"));
                let Ok(data) = std::fs::read(&path) else {
                    eprintln!("skip: no {} in {edition}", path.display());
                    continue;
                };
                let parsed = newh_res::parse(&data)
                    .unwrap_or_else(|error| panic!("{edition} {}: {error}", path.display()));
                assert_eq!(parsed.images.len(), RECORDS);
                let (w, h) = (parsed.images[0].width, parsed.images[0].height);
                let expected = width.parse::<u16>().unwrap();
                assert_eq!(
                    (w, h),
                    (expected, expected / 4 * 3),
                    "{edition} {path:?}: the background carries the resolution"
                );
            }
        }
    }
}

#[test]
fn the_hero_portraits_are_identical_between_editions() {
    let mut by_record: Vec<Option<Vec<u8>>> = vec![None; RECORDS];
    for (edition, game) in game_dirs() {
        let path = game.root().join("NEWH800R.RES");
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(_) => {
                eprintln!("skip: no {} in {edition}", path.display());
                continue;
            }
        };
        let parsed = newh_res::parse(&data).unwrap();
        for (record, image) in parsed.images.iter().enumerate() {
            let rgba = format!("{}x{}", image.width, image.height);
            let bytes = [rgba.as_bytes(), &image.rgba[..]].concat();
            match &by_record[record] {
                None => by_record[record] = Some(bytes),
                Some(previous) => {
                    assert_eq!(previous, &bytes, "record {record} differs between editions")
                }
            }
        }
    }
}

#[test]
fn the_hero_portrait_pairs_carry_six_images() {
    for (edition, game) in game_dirs() {
        let path = game.root().join("NEWH800R.RES");
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(_) => {
                eprintln!("skip: no {} in {edition}", path.display());
                continue;
            }
        };
        let parsed = newh_res::parse(&data).unwrap();
        for record in newh_res::PORTRAIT_RECORDS {
            for offset in [0, 1] {
                let image = &parsed.images[record + offset];
                assert_eq!(
                    (image.width, image.height),
                    (76, 87),
                    "{edition}: portrait pair {record}+{offset} is the card size"
                );
            }
        }
    }
}
