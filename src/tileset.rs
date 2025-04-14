use photo::{ImageRGBA, Transformation};

use crate::Tile;

/// A collection of all `Tile`'s which can be used to generate a `Map`.
pub struct Tileset {
    /// Size of the `Tile`s in pixels.
    tile_size: usize,
    /// Size of the `Tile` borders in pixels.
    border_size: usize,
    /// List of all `Tile`s in the `Tileset`.
    tiles: Vec<Tile>,
}

impl Tileset {
    /// Construct a new `Tileset` with a given tile size and border size.
    pub fn new(tile_size: usize, border_size: usize) -> Self {
        debug_assert!(tile_size > 0);
        debug_assert!(border_size > 0);

        Self {
            tile_size,
            border_size,
            tiles: Vec::new(),
        }
    }

    /// Get the inner tile size.
    pub fn tile_size(&self) -> usize {
        self.tile_size
    }

    /// Get the border size.
    pub fn border_size(&self) -> usize {
        self.border_size
    }

    /// Get the number of `Tile`s in the set.
    pub fn num_tiles(&self) -> usize {
        self.tiles.len()
    }

    /// Get a specific `Tile` by index.
    pub fn get_tile(&self, index: usize) -> &Tile {
        debug_assert!(index < self.tiles.len(), "Tile index out of bounds");
        self.tiles.get(index).unwrap()
    }

    /// Access the list of `Tile`s in the set.
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// Create a collection of `Tile`s from an image.
    pub fn cut_tiles(
        mut self,
        image: &ImageRGBA<u8>,
        overlap: usize,
        transforms: &[Transformation],
    ) -> Self {
        let cut_size = self.tile_size + (2 * self.border_size);

        debug_assert!(overlap < cut_size);
        debug_assert!(image.height() > cut_size);
        debug_assert!(image.width() > cut_size);

        debug_assert_eq!(
            (image.width() - overlap) % (cut_size - overlap),
            0,
            "Image must contain an integer number of tiles"
        );
        debug_assert_eq!(
            (image.height() - overlap) % (cut_size - overlap),
            0,
            "Image must contain an integer number of tiles"
        );

        let num_tiles_horizontal = (image.width() - overlap) / (cut_size - overlap);
        let num_tiles_vertical = (image.height() - overlap) / (cut_size - overlap);

        let step_size = cut_size - overlap;
        for y in (0..num_tiles_vertical).step_by(step_size) {
            for x in (0..num_tiles_horizontal).step_by(step_size) {
                let tile_image = image.extract([y, x], [cut_size, cut_size]);

                for transform in transforms {
                    // Apply the transformation to the tile image.
                    let transformed_tile_image = tile_image.transform(*transform);

                    // Check if the transformed tile image is already in the set, and if it is increase its frequency.
                    let mut new_tile = true;
                    for existing_tile in &mut self.tiles {
                        if existing_tile.image() == &transformed_tile_image {
                            existing_tile.increment_frequency();
                            new_tile = false;
                            break;
                        }
                    }
                    // Otherwise, add the a new tile image to the set.
                    if new_tile {
                        self.tiles.push(Tile::new(transformed_tile_image, 1));
                    }
                }
            }
        }

        self
    }
}
