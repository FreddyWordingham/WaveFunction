use bitvec::prelude::*;
use ndarray::{Array3, s};
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

/// `Rules` to guide `Tile` placement within a `Map`.
pub struct Rules {
    /// Relative frequencies which each `Tile` should be chosen during `Map` generation.
    frequencies: Vec<usize>,
    /// Rules for which `Tiles` can be placed adjacent to one another.
    adjacencies: Vec<[BitVec; 4]>,
}

impl Rules {
    /// Construct a new set of `Rules` from a list of frequencies and an adjacency matrix.
    pub fn new(frequencies: Vec<usize>, adjacency_matrix: &Array3<bool>) -> Self {
        debug_assert!(!frequencies.is_empty());
        let num_tiles = frequencies.len();
        debug_assert_eq!(adjacency_matrix.shape()[0], num_tiles);
        debug_assert_eq!(adjacency_matrix.shape()[1], num_tiles);
        debug_assert_eq!(adjacency_matrix.shape()[2], 2);

        Self {
            frequencies,
            adjacencies: Self::compile_adjacencies(adjacency_matrix),
        }
    }

    /// Load `Rules` from a file.
    pub fn load(path: &Path) {}

    /// Save the `Rules` to a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        write!(file, "{}", self)?;
        Ok(())
    }

    fn compile_adjacencies(adjacencies: &Array3<bool>) -> Vec<[BitVec; 4]> {
        debug_assert_eq!(adjacencies.shape()[0], adjacencies.shape()[1]);

        let num_tiles = adjacencies.shape()[0];
        (0..num_tiles)
            .map(|tile| {
                let east: BitVec = adjacencies.slice(s![tile, .., 0]).iter().cloned().collect();
                let west: BitVec = adjacencies.slice(s![.., tile, 0]).iter().cloned().collect();
                let north: BitVec = adjacencies.slice(s![tile, .., 1]).iter().cloned().collect();
                let south: BitVec = adjacencies.slice(s![.., tile, 1]).iter().cloned().collect();
                [east, west, north, south]
            })
            .collect()
    }

    /// Get the number of `Tiles`.
    pub fn num_tiles(&self) -> usize {
        self.frequencies.len()
    }

    /// Get the adjacency matrix.
    pub fn adjacency_matrix(&self) -> Array3<bool> {
        let num_tiles = self.frequencies.len();
        let mut adjacency_matrix = Array3::from_elem((num_tiles, num_tiles, 2), false);
        for i in 0..num_tiles {
            for j in 0..num_tiles {
                adjacency_matrix[[i, j, 0]] = self.adjacencies[i][0][j];
                adjacency_matrix[[i, j, 1]] = self.adjacencies[j][1][i];
            }
        }
        adjacency_matrix
    }
}

impl Display for Rules {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let width = self.frequencies.iter().max().unwrap().ilog10() as usize + 1;
        let num_tiles = self.frequencies.len();
        let adjacency_matrix = self.adjacency_matrix();

        for i in 0..num_tiles {
            // Write the frequency with padding.
            write!(f, "{:width$}", self.frequencies[i], width = width)?;
            // Write the adjacency rows for each dimension.
            for d in 0..adjacency_matrix.dim().2 {
                write!(f, "    ")?;
                for j in 0..num_tiles {
                    let ch = if adjacency_matrix[[i, j, d]] {
                        '1'
                    } else {
                        '0'
                    };
                    write!(f, "{}", ch)?;
                    if j < num_tiles - 1 {
                        write!(f, " ")?;
                    }
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
