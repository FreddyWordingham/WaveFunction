use anyhow::Result;
use photo::{ImageRGBA, Transformation};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

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

    /// Save the tile set to the specified directory.
    pub fn save(&self, output_dir: &Path) -> Result<()> {
        debug_assert!(self.tiles.len() > 0);
        debug_assert!(self.tiles.iter().all(|tile| tile.frequency() > 0));

        // Ensure the output directory exists.
        std::fs::create_dir_all(output_dir)?;

        // Save the tile frequencies to a separate file.
        let frequencies_path = output_dir.join("frequencies.txt");
        let mut frequencies_file = std::fs::File::create(frequencies_path)?;

        let image_filename_digits = ((self.tiles.len() - 1) as f64).log10().floor() as usize + 1;
        let image_frequency_digits = self
            .tiles
            .iter()
            .map(|tile| tile.frequency())
            .max()
            .unwrap()
            .ilog10() as usize
            + 1;

        for (i, tile) in self.tiles.iter().enumerate() {
            let file_name = format!("{:0width$}.png", i, width = image_filename_digits);
            let file_path: PathBuf = output_dir.join(file_name);
            tile.image().save(&file_path)?;

            let frequency = tile.frequency();
            writeln!(
                frequencies_file,
                "{} {:width$}",
                file_path.file_name().expect("").to_string_lossy(),
                frequency,
                width = image_frequency_digits
            )?;
        }

        Ok(())
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

    /// Add `Tile`s to the `Tileset` from an image.
    pub fn add_tiles(
        mut self,
        image: &ImageRGBA<u8>,
        overlap: usize,
        transforms: &[Transformation],
    ) -> Self {
        let cut_size = self.tile_size + (2 * self.border_size);
        println!(
            "Cutting tiles of size {} with overlap {}",
            cut_size, overlap
        );
        for new_image in image.extract_tiles(cut_size, overlap) {
            for &transform in transforms {
                let transformed_image = new_image.transform(transform);
                // Look for an existing tile with the same image.
                if let Some(existing_tile) = self
                    .tiles
                    .iter_mut()
                    .find(|tile| tile.image() == &transformed_image)
                {
                    existing_tile.increment_frequency();
                } else {
                    self.tiles.push(Tile::new(transformed_image.clone(), 1));
                }
            }
        }
        self
    }
}
