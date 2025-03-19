use photo::{Direction, ImageError, ImageRGBA, Transformation};

use crate::{Rule, RuleSet};

pub struct TileSet {
    tile_size: usize,
    border_size: usize,
    tiles: Vec<ImageRGBA<u8>>,
}

impl TileSet {
    pub fn new(tile_size: usize, border_size: usize) -> Self {
        debug_assert!(tile_size > 0);
        debug_assert!(border_size > 0);

        Self {
            tile_size,
            border_size,
            tiles: Vec::new(),
        }
    }

    /// Load a tile set from a directory of PNG files in alphabetical order.
    pub fn load(tile_size: usize, border_size: usize, input_dir: &str) -> Result<Self, ImageError> {
        // List files in the given directory.
        let mut files: Vec<String> = std::fs::read_dir(input_dir)?
            .map(|entry| entry.unwrap().path().display().to_string())
            .collect();
        files.sort();

        // Load the images and verify their dimensions.
        let image_size = tile_size + (2 * border_size);
        let mut tiles = Vec::with_capacity(files.len());
        for file in files.iter() {
            let image = ImageRGBA::<u8>::load(file)?;
            assert!(
                image.height() == image_size,
                "Image dimensions do not match the desired tile size."
            );
            assert!(
                image.width() == image_size,
                "Image dimensions do not match the desired tile size."
            );
            tiles.push(image);
        }

        Ok(Self {
            tile_size,
            border_size,
            tiles,
        })
    }

    pub fn save(&self, output_dir: &str) -> Result<(), ImageError> {
        let num_tiles = self.tiles.len();
        let digits = (num_tiles as f64).log10().ceil() as usize;

        for (i, tile) in self.tiles.iter().enumerate() {
            let filename = format!("{}/{:0width$}.png", output_dir, i, width = digits);
            tile.save(&filename)?;
        }
        Ok(())
    }

    pub fn num_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn ingest(mut self, map: &ImageRGBA<u8>) -> Self {
        let height = map.height();
        let width = map.width();

        debug_assert!(
            (height - (2 * self.border_size)) % self.tile_size == 0,
            "Example map image must have a height that is a multiple of the tile size plus the 2x border size."
        );
        debug_assert!(
            (width - (2 * self.border_size)) % self.tile_size == 0,
            "Example map image must have a width that is a multiple of the tile size plus the 2x border size."
        );

        // Iterate over the map in tile-sized chunks, with an offset and overlap for the border.
        for y in (0..(height - (2 * self.border_size))).step_by(self.tile_size) {
            for x in (0..(width - (2 * self.border_size))).step_by(self.tile_size) {
                // Extract the tile.
                let tile = map.extract(
                    [y, x],
                    [
                        self.tile_size + (2 * self.border_size),
                        (self.tile_size + (2 * self.border_size)),
                    ],
                );

                // Check if the tile image is already in the set,
                // and increase its frequency if it is.
                let mut new_tile = true;
                for existing_tile in &self.tiles {
                    if existing_tile.data == tile.data {
                        new_tile = false;
                        break;
                    }
                }
                // Otherwise, add the tile image to the set.
                if new_tile {
                    self.tiles.push(tile);
                }
            }
        }

        self
    }

    /// Apply the given transformations to each tile in the set, and create a new tile entry for each unique transformation.
    pub fn with_transformations(mut self, transfomations: &[Transformation]) -> Self {
        let mut new_tiles: Vec<ImageRGBA<u8>> = Vec::new();

        for tile in self.tiles.iter() {
            // Check if the tile is already in the set.
            let mut new_tile = true;
            for existing_tile in new_tiles.iter_mut() {
                if existing_tile.data == tile.data {
                    new_tile = false;
                    break;
                }
            }

            // Otherwise, add the tile to the set.
            if new_tile {
                new_tiles.push(tile.clone());
            }

            // Apply the transformations to the tile and add any new tiles to the set.
            for transformation in transfomations.iter() {
                // Skip the identity transformation.
                if *transformation == Transformation::Identity {
                    continue;
                }

                let transformed_tile = tile.transform(*transformation);
                let mut new_tile = true;
                for existing_tile in new_tiles.iter_mut() {
                    if existing_tile.data == transformed_tile.data {
                        new_tile = false;
                        break;
                    }
                }
                if new_tile {
                    new_tiles.push(transformed_tile);
                }
            }
        }

        self.tiles = new_tiles;
        self
    }

    /// Determine the adjacency rules for each tile in the set.
    pub fn generate_rules(&self) -> RuleSet {
        let mut rules = Vec::with_capacity(self.num_tiles());

        for tile in &self.tiles {
            let mut north_adjacent = Vec::new();
            let mut east_adjacent = Vec::new();
            let mut south_adjacent = Vec::new();
            let mut west_adjacent = Vec::new();

            for (n, other_tile) in self.tiles.iter().enumerate() {
                if other_tile.view_border(Direction::South, self.border_size)
                    == tile.view_border(Direction::North, self.border_size)
                {
                    north_adjacent.push(n);
                }
                if other_tile.view_border(Direction::West, self.border_size)
                    == tile.view_border(Direction::East, self.border_size)
                {
                    east_adjacent.push(n);
                }
                if other_tile.view_border(Direction::North, self.border_size)
                    == tile.view_border(Direction::South, self.border_size)
                {
                    south_adjacent.push(n);
                }
                if other_tile.view_border(Direction::East, self.border_size)
                    == tile.view_border(Direction::West, self.border_size)
                {
                    west_adjacent.push(n);
                }
            }

            rules.push(Rule::new(
                north_adjacent,
                east_adjacent,
                south_adjacent,
                west_adjacent,
            ));
        }

        RuleSet::new(rules)
    }
}
