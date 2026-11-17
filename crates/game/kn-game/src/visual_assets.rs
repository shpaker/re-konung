//! Optional visual assets exposed as decoded data for frontend adapters.

use std::path::{Path, PathBuf};

use k1_data::{lights_res, map, newh_res};
use k1_game::resources::find_named;
use thiserror::Error;

use crate::{Game, MapRef, Rgba};

pub type LightMask = (Vec<u8>, u32, u32);

#[derive(Debug, Error)]
pub enum LightMaskError {
    #[error("LIGHTS.RES not found in {}; the scene lights without tile masks", root.display())]
    Missing { root: PathBuf },
    #[error("LIGHTS.RES unreadable at {}: {source}; the scene lights without tile masks", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("LIGHTS.RES unparsable at {}: {reason}; the scene lights without tile masks", path.display())]
    Parse { path: PathBuf, reason: String },
}

impl Game {
    /// Reads the current file again, independently of the resources loaded at open.
    pub fn light_masks(&self) -> std::result::Result<Vec<LightMask>, LightMaskError> {
        light_masks(self.root())
    }

    pub fn map_light_grid(&self, map_ref: MapRef) -> Option<Vec<Vec<u8>>> {
        map_light_grid(&self.dir, map_ref)
    }

    pub fn menu_portraits(&self) -> Option<Vec<(u8, Rgba)>> {
        menu_portraits(self.root())
    }
}

fn light_masks(root: &Path) -> std::result::Result<Vec<LightMask>, LightMaskError> {
    let path = find_named(root, "lights.res").ok_or_else(|| LightMaskError::Missing {
        root: root.to_path_buf(),
    })?;
    let bytes = std::fs::read(&path).map_err(|source| LightMaskError::Read {
        path: path.clone(),
        source,
    })?;
    let resource = lights_res::parse(&bytes).map_err(|error| LightMaskError::Parse {
        path,
        reason: error.to_string(),
    })?;
    Ok(resource
        .tiles
        .into_iter()
        .map(|mask| {
            (
                mask,
                lights_res::TILE_MASK_WIDTH as u32,
                lights_res::TILE_MASK_HEIGHT as u32,
            )
        })
        .collect())
}

fn map_light_grid(dir: &k1_data::repo::GameDir, map_ref: MapRef) -> Option<Vec<Vec<u8>>> {
    let bytes = dir
        .lights_bytes(map_ref.number, map_ref.source.file_set())
        .ok()??;
    map::parse_lights(&bytes).ok().map(|lights| lights.fine)
}

fn menu_portraits(root: &Path) -> Option<Vec<(u8, Rgba)>> {
    let resource = ["NEWH800R.RES", "NEWH800E.RES"]
        .into_iter()
        .find_map(|name| {
            let bytes = std::fs::read(root.join(name)).ok()?;
            newh_res::parse(&bytes).ok()
        })?;
    Some(
        resource
            .images
            .into_iter()
            .enumerate()
            .skip(3)
            .take(6)
            .map(|(record, image)| {
                (
                    record as u8,
                    Rgba {
                        width: u32::from(image.width),
                        height: u32::from(image.height),
                        data: image.rgba,
                    },
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MapSource;
    use k1_data::repo::GameDir;

    #[test]
    fn light_mask_failures_keep_their_category_and_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            matches!(light_masks(dir.path()), Err(LightMaskError::Missing { root }) if root == dir.path())
        );
        let path = dir.path().join("LiGhTs.ReS");
        std::fs::create_dir(&path).unwrap();
        assert!(
            matches!(light_masks(dir.path()), Err(LightMaskError::Read { path: actual, .. }) if actual == path)
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, b"invalid").unwrap();
        assert!(
            matches!(light_masks(dir.path()), Err(LightMaskError::Parse { path: actual, .. }) if actual == path)
        );
    }

    #[test]
    fn mask_loading_observes_file_changes_and_preserves_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("LIGHTS.RES");
        std::fs::write(&path, vec![7; lights_res::SIZE]).unwrap();
        let first = light_masks(dir.path()).unwrap();
        assert_eq!(first.len(), 19);
        assert_eq!(
            (first[0].1, first[0].2, first[0].0.len()),
            (114, 64, 114 * 64)
        );
        assert!(first[0].0.iter().all(|value| *value == 7));
        std::fs::write(&path, vec![9; lights_res::SIZE]).unwrap();
        assert!(
            light_masks(dir.path()).unwrap()[0]
                .0
                .iter()
                .all(|value| *value == 9)
        );
    }

    #[test]
    fn fine_grid_loading_selects_the_source_and_tolerates_missing_or_invalid_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = GameDir::new(root.path());
        let game = MapRef {
            source: MapSource::Game,
            number: 4,
        };
        let tutorial = MapRef {
            source: MapSource::Tutorial,
            number: 4,
        };
        assert_eq!(map_light_grid(&dir, game), None);
        std::fs::write(root.path().join("LIGHTS.4"), b"invalid").unwrap();
        assert_eq!(map_light_grid(&dir, game), None);
        std::fs::write(root.path().join("LIGHTS.4"), vec![3; map::LIGHTS_SIZE]).unwrap();
        std::fs::write(root.path().join("TLIGHTS.4"), vec![5; map::LIGHTS_SIZE]).unwrap();
        let normal = map_light_grid(&dir, game).unwrap();
        let training = map_light_grid(&dir, tutorial).unwrap();
        assert_eq!((normal.len(), normal[0].len()), (320, 160));
        assert!(normal.iter().flatten().all(|value| *value == 3));
        assert!(training.iter().flatten().all(|value| *value == 5));
    }

    fn portrait_file(color: u16) -> Vec<u8> {
        let mut file = Vec::new();
        for _ in 0..13 {
            let mut image = vec![1, 0, 1, 0, 0, 0, 1];
            image.extend_from_slice(&color.to_le_bytes());
            image.push(0);
            file.extend_from_slice(&[0; 4]);
            file.extend_from_slice(&(image.len() as u32).to_le_bytes());
            file.extend_from_slice(&image);
        }
        file
    }

    #[test]
    fn portraits_fall_back_on_read_or_parse_failure_and_keep_record_ids() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(menu_portraits(dir.path()), None);
        std::fs::write(dir.path().join("NEWH800E.RES"), portrait_file(31)).unwrap();
        for russian in [None, Some(b"invalid".as_slice())] {
            if let Some(bytes) = russian {
                std::fs::write(dir.path().join("NEWH800R.RES"), bytes).unwrap();
            }
            let portraits = menu_portraits(dir.path()).unwrap();
            assert_eq!(
                portraits.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![3, 4, 5, 6, 7, 8]
            );
            assert!(portraits.iter().all(|(_, image)| image.width == 1
                && image.height == 1
                && image.data == [0, 0, 255, 255]));
        }
        std::fs::write(dir.path().join("NEWH800R.RES"), portrait_file(31 << 10)).unwrap();
        assert_eq!(
            menu_portraits(dir.path()).unwrap()[0].1.data,
            [255, 0, 0, 255]
        );
    }
}
