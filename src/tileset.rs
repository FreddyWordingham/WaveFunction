use anyhow::Result;
use ndarray::Array3;
use photo::ImageRGBA;
use std::{env, io::Write, path::Path};

use crate::Rules;

const TILESET_FILENAME: &str = "tiles.txt";
const ADJACENCY_INVALID_SYMBOL: &str = "0";
const ADJACENCY_VALID_SYMBOL: &str = "1";

pub struct Tileset {
    interior_size: usize,
    border_size: usize,
    tiles: Vec<(ImageRGBA<u8>, usize)>,
    rules: Rules,
}

impl Tileset {
    pub fn new(
        interior_size: usize,
        border_size: usize,
        tiles: Vec<(ImageRGBA<u8>, usize)>,
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
            rules,
        }
    }

    pub fn from_str(interior_size: usize, border_size: usize, data: &str) -> Self {
        debug_assert!(interior_size > 0, "Interior size must be greater than 0");
        debug_assert!(border_size > 0, "Border size must be greater than 0");

        // Read line by line, ignoring empty lines and comments
        let lines = data
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .collect::<Vec<_>>();

        let num_tiles = lines.len();
        let mut tiles = Vec::with_capacity(num_tiles);
        let mut adjacency_matrix = Array3::from_elem((num_tiles, num_tiles, 2), false);

        for (n, line) in lines.iter().enumerate() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 2 + (2 * num_tiles) {
                panic!("Invalid line format: {}", line);
            }

            let tile = ImageRGBA::<u8>::load(parts[0]).expect("Failed to load tile image");
            let frequency = parts[1].parse::<usize>().expect("Invalid frequency");
            tiles.push((tile, frequency));

            // Parse the adjacency matrix
            for i in 0..num_tiles {
                adjacency_matrix[(n, i, 0)] = parts[2 + i] == ADJACENCY_VALID_SYMBOL;
            }
            for i in 0..num_tiles {
                adjacency_matrix[(n, i, 1)] = parts[2 + num_tiles + i] == ADJACENCY_VALID_SYMBOL;
            }
        }

        Self {
            interior_size,
            border_size,
            tiles,
            rules: Rules::new(adjacency_matrix),
        }
    }

    pub fn load(interior_size: usize, border_size: usize, path: &Path) -> Self {
        debug_assert!(path.is_file(), "Path must be a file");
        let data = std::fs::read_to_string(path).expect("Failed to read file");
        Self::from_str(interior_size, border_size, &data)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        assert!(!path.is_file(), "Path must be a directory");
        debug_assert!(
            !self.tiles.is_empty(),
            "TilesetBuilder must contain at least one tile before it can be saved"
        );

        // Create the directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }

        // Calculate the print widths for the index and frequency
        let index_print_width = self.tiles.len().to_string().len();
        let frequency_print_width = self
            .max_frequency()
            .expect("No tiles found")
            .to_string()
            .len();

        let adjacency_matrix = self.rules.adjacency_matrix();
        let cwd = env::current_dir()?.canonicalize()?;

        // Save the frequencies and tiles to the specified directory
        let frequencies_path = path.join(TILESET_FILENAME);
        let mut frequencies_file = std::fs::File::create(frequencies_path)?;
        for (i, (tile, frequency)) in self.tiles.iter().enumerate() {
            let tile_filename = format!("{i:0index_print_width$}.png");
            let tile_path = path.join(&tile_filename);
            tile.save(&tile_path)?;

            let abs_tile_path = tile_path.canonicalize()?;
            let relative_tile_path = abs_tile_path.strip_prefix(&cwd).unwrap_or(&abs_tile_path);

            write!(
                frequencies_file,
                "{}    {frequency:frequency_print_width$}    ",
                Path::new(".").join(relative_tile_path).display()
            )?;

            for j in 0..self.len() {
                if adjacency_matrix[[i, j, 0]] {
                    write!(frequencies_file, "{} ", ADJACENCY_VALID_SYMBOL)?;
                } else {
                    write!(frequencies_file, "{} ", ADJACENCY_INVALID_SYMBOL)?;
                }
            }
            write!(frequencies_file, "   ")?;
            for j in 0..self.len() {
                if adjacency_matrix[[i, j, 1]] {
                    write!(frequencies_file, "{} ", ADJACENCY_VALID_SYMBOL)?;
                } else {
                    write!(frequencies_file, "{} ", ADJACENCY_INVALID_SYMBOL)?;
                }
            }

            writeln!(frequencies_file)?;
        }
        Ok(())
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

    pub fn tiles(&self) -> &[(ImageRGBA<u8>, usize)] {
        &self.tiles
    }

    pub fn weights(&self) -> Vec<usize> {
        self.tiles.iter().map(|tile| tile.1).collect::<Vec<_>>()
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn frequency(&self, index: usize) -> usize {
        debug_assert!(
            index < self.tiles.len(),
            "Index out of bounds: {} >= {}",
            index,
            self.tiles.len()
        );
        self.tiles[index].1
    }

    fn max_frequency(&self) -> Option<usize> {
        self.tiles.iter().map(|tile| tile.1).max()
    }

    pub fn interiors(&self) -> Vec<ImageRGBA<u8>> {
        self.tiles
            .iter()
            .map(|tile| tile.0.interior(self.border_size))
            .collect()
    }
}
