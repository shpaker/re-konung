//! Allocation constants checked against both original executables, not Slavik tables.
use k1_game::new_game::{COST_BANDS, LIMITS};
use std::path::PathBuf;

#[test]
fn allocation_tables_match_both_executables() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    for (edition, executable, costs, limits) in [
        ("rus", "konung.res", 0x411b0, 0x412ac),
        ("eng", "KONUNG.EXE", 0x423e8, 0x424e4),
    ] {
        let path = root.join(edition).join("game").join(executable);
        if !path.exists() {
            continue;
        }
        let data = std::fs::read(path).unwrap();
        let expected: Vec<u8> = COST_BANDS
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        assert_eq!(
            &data[costs..costs + expected.len()],
            expected,
            "{edition} cost bands"
        );
        for (row, class_index) in [4, 2, 1, 3].into_iter().enumerate() {
            for attribute in 0..4 {
                // Class-major layout; current-level maximum is at 2*level-1.
                assert_eq!(
                    i16::from(data[limits + class_index * 104 + attribute * 26 + 19]),
                    LIMITS[row][attribute],
                    "{edition}: class {row}, attribute {attribute}"
                );
            }
        }
    }
}
