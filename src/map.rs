use anyhow::Result;
use fixedbitset::FixedBitSet;
use ndarray::Array2;
use photo::ImageRGBA;
use rand::Rng;
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io::Write,
    ops::{Index, IndexMut},
};

use crate::{Cell, Rules, Tileset, WaveFunction};

const WILDCARD_COLOUR: [u8; 4] = [255, 0, 255, 255];
const IGNORE_COLOUR: [u8; 4] = [0, 0, 0, 0];

#[derive(Clone)]
pub struct Map {
    cells: Array2<Cell>,
}

impl Map {
    pub fn new(cells: Array2<Cell>) -> Self {
        debug_assert!(!cells.is_empty(), "Cell map must contain at least one cell");
        Self { cells }
    }

    pub fn empty(resolution: (usize, usize)) -> Self {
        debug_assert!(resolution.0 > 0, "Map height must be greater than zero");
        debug_assert!(resolution.1 > 0, "Map width must be greater than zero");
        let cells = Array2::from_elem(resolution, Cell::Wildcard);
        Self { cells }
    }

    pub fn from_str(map_str: &str) -> Self {
        let cells: Vec<Vec<Cell>> = map_str
            .lines()
            .map(|line| line.trim()) // Remove surrounding whitespace
            .filter(|line| !line.is_empty() && !line.starts_with('#')) // Skip blank or commented lines
            .map(|line| {
                line.split_whitespace()
                    .map(|cell_str| Cell::from(cell_str))
                    .collect()
            })
            .collect();

        let height = cells.len();
        let width = if height > 0 { cells[0].len() } else { 0 };
        cells.iter().for_each(|row| {
            assert_eq!(row.len(), width, "All rows must have the same length");
        });

        Self::new(
            Array2::from_shape_vec((height, width), cells.into_iter().flatten().collect())
                .expect("Failed to create cell array"),
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
        self.cells
            .iter()
            .filter_map(|cell| match cell {
                Cell::Fixed(index) => Some(*index),
                Cell::Ignore => None,
                Cell::Wildcard => None,
            })
            .max()
    }

    pub fn size(&self) -> (usize, usize) {
        self.cells.dim()
    }

    pub fn mask(&self) -> Array2<bool> {
        self.cells.mapv(|cell| match cell {
            Cell::Ignore => true,
            Cell::Wildcard => false,
            Cell::Fixed(_) => false,
        })
    }

    pub fn domains(&self, num_tiles: usize) -> Array2<FixedBitSet> {
        self.cells.mapv(|cell| cell.domain(num_tiles))
    }

    pub fn collapse<WF: WaveFunction>(&self, rules: &Rules, rng: &mut impl Rng) -> Result<Self> {
        WF::collapse(self, rules, rng)
    }

    pub fn render(&self, tileset: &Tileset) -> ImageRGBA<u8> {
        debug_assert!(
            self.max_index().map_or(true, |index| index < tileset.len()),
            "Index out of bounds for tileset"
        );
        let interiors = tileset.interiors();
        let interior_size = tileset.interior_size();
        let wildcard_img = ImageRGBA::filled([interior_size, interior_size], WILDCARD_COLOUR);
        let ignore_img = ImageRGBA::filled([interior_size, interior_size], IGNORE_COLOUR);
        let data = self.cells.mapv(|cell| match cell {
            Cell::Fixed(index) => interiors[index].clone(),
            Cell::Ignore => ignore_img.clone(),
            Cell::Wildcard => wildcard_img.clone(),
        });

        ImageRGBA::from_tiles(&data)
    }
}

impl Index<(usize, usize)> for Map {
    type Output = Cell;

    fn index(&self, idx: (usize, usize)) -> &Self::Output {
        debug_assert!(
            idx.0 < self.cells.shape()[0],
            "Index out of bounds for map height"
        );
        debug_assert!(
            idx.1 < self.cells.shape()[1],
            "Index out of bounds for map width"
        );
        &self.cells[idx]
    }
}

impl IndexMut<(usize, usize)> for Map {
    fn index_mut(&mut self, idx: (usize, usize)) -> &mut Self::Output {
        debug_assert!(
            idx.0 < self.cells.shape()[0],
            "Index out of bounds for map height"
        );
        debug_assert!(
            idx.1 < self.cells.shape()[1],
            "Index out of bounds for map width"
        );
        &mut self.cells[idx]
    }
}

impl Display for Map {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let print_width = self.max_index().unwrap_or(0).to_string().len();
        for row in self.cells.rows() {
            for cell in row.iter() {
                let s = &format!("{}", cell);
                write!(f, "{s:>print_width$} ")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
