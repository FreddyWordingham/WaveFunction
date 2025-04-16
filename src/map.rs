use ndarray::Array2;
use photo::ImageRGBA;
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io::Write,
};

use crate::{Tile, Tileset};

pub struct Map {
    tiles: Array2<Tile>,
}

impl Map {
    pub fn new(tiles: Array2<Tile>) -> Self {
        debug_assert!(!tiles.is_empty(), "Tile map must contain at least one tile");
        Self { tiles }
    }

    pub fn empty(resolution: (usize, usize)) -> Self {
        debug_assert!(resolution.0 > 0, "Map height must be greater than zero");
        debug_assert!(resolution.1 > 0, "Map width must be greater than zero");
        let tiles = Array2::from_elem(resolution, Tile::Wildcard);
        Self { tiles }
    }

    pub fn from_str(map_str: &str) -> Self {
        let tiles: Vec<Vec<Tile>> = map_str
            .lines()
            .map(|line| line.trim()) // Remove surrounding whitespace
            .filter(|line| !line.is_empty() && !line.starts_with('#')) // Skip blank or commented lines
            .map(|line| {
                line.split_whitespace()
                    .map(|tile_str| Tile::from(tile_str))
                    .collect()
            })
            .collect();

        let height = tiles.len();
        let width = if height > 0 { tiles[0].len() } else { 0 };
        tiles.iter().for_each(|row| {
            assert_eq!(row.len(), width, "All rows must have the same length");
        });

        Self::new(
            Array2::from_shape_vec((height, width), tiles.into_iter().flatten().collect())
                .expect("Failed to create tile array"),
        )
    }

    pub fn load(path: &str) -> std::io::Result<Self> {
        let map_str = std::fs::read_to_string(path)?;
        Ok(Self::from_str(&map_str))
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        write!(file, "{}", self)?;
        Ok(())
    }

    pub fn max_index(&self) -> Option<usize> {
        self.tiles
            .iter()
            .filter_map(|tile| match tile {
                Tile::Fixed(index) => Some(*index),
                Tile::Ignore => None,
                Tile::Wildcard => None,
            })
            .max()
    }

    pub fn tiles(&self) -> &Array2<Tile> {
        &self.tiles
    }

    pub fn get(&self, index: (usize, usize)) -> Tile {
        debug_assert!(
            index.0 < self.tiles.shape()[0],
            "Index out of bounds for map height"
        );
        debug_assert!(
            index.1 < self.tiles.shape()[1],
            "Index out of bounds for map width"
        );
        self.tiles[index].clone()
    }

    pub fn set(&mut self, index: (usize, usize), tile: Tile) {
        debug_assert!(
            index.0 < self.tiles.shape()[0],
            "Index out of bounds for map height"
        );
        debug_assert!(
            index.1 < self.tiles.shape()[1],
            "Index out of bounds for map width"
        );
        self.tiles[index] = tile;
    }

    pub fn render(&self, tileset: &Tileset) -> ImageRGBA<u8> {
        debug_assert!(
            self.max_index().map_or(true, |index| index < tileset.len()),
            "Tile index out of bounds for tileset"
        );
        unimplemented!()
    }
}

impl Display for Map {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let print_width = self.max_index().unwrap_or(0).ilog10() as usize + 1;
        for row in self.tiles.rows() {
            for tile in row.iter() {
                write!(f, "{tile:print_width$} ")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
