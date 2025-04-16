use anyhow::Result;
use photo::{ImageRGBA, Transformation};
use std::{io::Write, path::Path};

use crate::Tileset;

const FREQUENCIES_FILENAME: &str = "frequencies.txt";

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

    /// Save the `TilesetBuilder` to the given directory.
    pub fn save(&self, path: &Path) -> Result<()> {
        assert!(!path.is_file(), "Path must be a directory");

        // Create the directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }

        // Save the frequencies to a file
        let max_frequency = self
            .tiles
            .iter()
            .map(|(_, frequency)| *frequency)
            .max()
            .unwrap_or(0);
        let width = max_frequency.to_string().len();

        let frequencies_path = path.join(FREQUENCIES_FILENAME);
        let mut frequencies_file = std::fs::File::create(frequencies_path)?;
        for (i, (tile, frequency)) in self.tiles.iter().enumerate() {
            let tile_path = path.join(format!("{0:width$}.png", i));
            tile.save(&tile_path)?;
            writeln!(
                frequencies_file,
                "{} {:width$}",
                tile_path.display(),
                frequency
            )?;
        }

        // Save the tiles to individual files
        for (tile, frequency) in &self.tiles {
            let tile_path = path.join(format!("{}.png", frequency));
            tile.save(&tile_path)?;
        }

        Ok(())
    }

    pub fn interior_size(&self) -> usize {
        self.interior_size
    }

    pub fn border_size(&self) -> usize {
        self.border_size
    }

    pub fn tile_size(&self) -> usize {
        self.interior_size + (2 * self.border_size)
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
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

    pub fn build() -> Tileset {
        unimplemented!()
    }
}
