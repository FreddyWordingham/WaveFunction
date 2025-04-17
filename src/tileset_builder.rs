use ndarray::Array3;
use photo::{Direction, ImageRGBA, Transformation};

use crate::{Rules, Tileset};

pub struct TilesetBuilder {
    interior_size: usize,
    border_size: usize,
    tiles: Vec<(ImageRGBA<u8>, usize)>,
}

impl TilesetBuilder {
    pub fn new(interior_size: usize, border_size: usize) -> Self {
        debug_assert!(interior_size > 0, "Interior size must be greater than 0");
        debug_assert!(border_size > 0, "Border size must be greater than 0");
        Self {
            interior_size,
            border_size,
            tiles: Vec::new(),
        }
    }

    pub fn interior_size(&self) -> usize {
        self.interior_size
    }

    pub fn border_size(&self) -> usize {
        self.border_size
    }

    pub fn tiles(&self) -> &[(ImageRGBA<u8>, usize)] {
        &self.tiles
    }

    pub fn tile_size(&self) -> usize {
        self.interior_size + (2 * self.border_size)
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    fn adjacency_matrix(&self) -> Array3<bool> {
        debug_assert!(
            !self.tiles.is_empty(),
            "TilesetBuilder must contain at least one tile before it can be built"
        );
        let mut adjacent = Array3::from_elem((self.len(), self.len(), 2), false);
        for (self_index, self_tile) in self.tiles.iter().enumerate() {
            for (other_index, other_tile) in self.tiles.iter().enumerate() {
                if self_tile.0.view_border(Direction::East, self.border_size)
                    == other_tile.0.view_border(Direction::West, self.border_size)
                {
                    adjacent[[self_index, other_index, 0]] = true;
                }
                if self_tile.0.view_border(Direction::North, self.border_size)
                    == other_tile.0.view_border(Direction::South, self.border_size)
                {
                    adjacent[[self_index, other_index, 1]] = true;
                }
            }
        }
        adjacent
    }

    pub fn add_tiles(
        mut self,
        image: &ImageRGBA<u8>,
        overlap: usize,
        transformations: &[Transformation],
    ) -> Self {
        for new_image in image.extract_tiles(self.tile_size(), overlap) {
            for &transform in transformations {
                let transformed_image = new_image.transform(transform);
                if let Some(index) = self
                    .tiles
                    .iter()
                    .position(|tile| tile.0 == transformed_image)
                {
                    self.tiles[index].1 += 1;
                } else {
                    self.tiles.push((transformed_image, 1));
                }
            }
        }
        self
    }

    pub fn build(self) -> Tileset {
        debug_assert!(
            !self.tiles.is_empty(),
            "TilesetBuilder must contain at least one tile before it can be built"
        );
        let rules = Rules::new(self.adjacency_matrix());
        Tileset::new(self.interior_size, self.border_size, self.tiles, rules)
    }
}
