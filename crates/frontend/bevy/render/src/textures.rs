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
    /// Докачивает недостающие ключи; возвращает текст проблем по тем, что не
    /// собрались. Пустой список — все ключи дали картинку.
    ///
    /// Кусок с несобравшейся текстурой не рисуется, но причина остаётся
    /// видимой: молча потерять спрайт и показать аккуратную, но неполную карту
    /// хуже, чем сказать, что данные битые.
    pub fn load(
        &mut self,
        game: &Game,
        ids: &[TextureId],
        images: &mut Assets<Image>,
    ) -> kn_game::Result<Vec<String>> {
        let missing: Vec<TextureId> = ids
            .iter()
            .copied()
            .filter(|id| !self.entries.contains_key(id))
            .collect();
        if missing.is_empty() {
            return Ok(Vec::new());
        }
        let decoded = game.textures(&missing)?;
        let mut problems = Vec::new();
        for (id, decoded) in missing.into_iter().zip(decoded) {
            match decoded {
                Ok(Some(rgba)) => {
                    let size = UVec2::new(rgba.width, rgba.height);
                    let handle = images.add(to_image(&rgba));
                    self.entries.insert(id, TextureEntry { handle, size });
                }
                Ok(None) => problems.push(format!("{id:?}: пустой слот")),
                Err(error) => problems.push(format!("{id:?}: {error}")),
            }
        }
        Ok(problems)
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
