use photo::ImageRGBA;

use crate::Rules;

pub struct Tileset {
    interior_size: usize,
    border_size: usize,
    tiles: Vec<ImageRGBA<u8>>,
    _rules: Rules,
}

impl Tileset {
    pub fn new(
        interior_size: usize,
        border_size: usize,
        tiles: Vec<ImageRGBA<u8>>,
        rules: Rules,
    ) -> Self {
        debug_assert!(interior_size > 0, "Interior size must be greater than 0");
        debug_assert!(border_size > 0, "Border size must be greater than 0");
        debug_assert!(!tiles.is_empty(), "Tileset must contain at least one tile");
        debug_assert!(
            tiles.len() == rules.len(),
            "Number of tiles must match number of rules"
        );

        Self {
            interior_size,
            border_size,
            tiles,
            _rules: rules,
        }
    }

    pub fn interior_size(&self) -> usize {
        self.interior_size
    }

    pub fn border_size(&self) -> usize {
        self.border_size
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn glyphs(&self) -> Vec<ImageRGBA<u8>> {
        self.tiles
            .iter()
            .map(|tile| tile.interior(self.border_size))
            .collect()
    }
}
