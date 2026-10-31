//! Кэш текстур: RGBA из [`kn_game`] → [`Image`], по одному ассету на ключ.
//!
//! Уникальных картинок на карте в разы меньше кусков (тайлов земли не больше
//! 512, кадры объектов и спрайты повторяются), поэтому спрайты делят
//! `Handle<Image>` одного ключа.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use kn_game::{Game, Rgba, TextureId};

/// Ассет картинки и её размер в пикселях.
#[derive(Debug, Clone)]
pub struct TextureEntry {
    pub handle: Handle<Image>,
    pub size: UVec2,
}

/// Загруженные текстуры: ключ → ассет. Переиспользуется между картами.
#[derive(Resource, Default)]
pub struct TextureCache {
    entries: HashMap<TextureId, TextureEntry>,
}

impl TextureCache {
    /// Докачивает недостающие ключи; возвращает число новых текстур.
    ///
    /// Ключи без данных (пустой слот, выход за границы) пропускаются —
    /// `kn_game::Game::check` сообщает о них отдельно.
    pub fn load(
        &mut self,
        game: &Game,
        ids: &[TextureId],
        images: &mut Assets<Image>,
    ) -> kn_game::Result<usize> {
        let missing: Vec<TextureId> = ids
            .iter()
            .copied()
            .filter(|id| !self.entries.contains_key(id))
            .collect();
        if missing.is_empty() {
            return Ok(0);
        }
        let decoded = game.textures(&missing)?;
        let mut added = 0;
        for (id, rgba) in missing.into_iter().zip(decoded) {
            let Some(rgba) = rgba else { continue };
            let size = UVec2::new(rgba.width, rgba.height);
            let handle = images.add(to_image(&rgba));
            self.entries.insert(id, TextureEntry { handle, size });
            added += 1;
        }
        Ok(added)
    }

    pub fn get(&self, id: &TextureId) -> Option<&TextureEntry> {
        self.entries.get(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// RGBA-байты в текстуру; только для рендера — читать их обратно не нужно.
fn to_image(rgba: &Rgba) -> Image {
    Image::new(
        Extent3d {
            width: rgba.width.max(1),
            height: rgba.height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.data.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_size_matches_rgba() {
        let image = to_image(&Rgba {
            width: 3,
            height: 2,
            data: vec![0u8; 3 * 2 * 4],
        });
        assert_eq!(image.size(), UVec2::new(3, 2));
    }

    #[test]
    fn empty_cache_has_no_entries() {
        let cache = TextureCache::default();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
