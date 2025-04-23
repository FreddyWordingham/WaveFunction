use fixedbitset::FixedBitSet;
use ndarray::Array3;
use photo::Direction;
use std::ops::Index;

pub struct Rules {
    masks: Vec<[FixedBitSet; 4]>, // [N, E, S, W]
    frequencies: Vec<usize>,
}

impl Rules {
    pub fn new(adjacency_matrix: Array3<bool>, frequencies: Vec<usize>) -> Self {
        assert!(
            frequencies.iter().all(|&f| f > 0),
            "Frequencies must be positive"
        );
        let num_tiles = frequencies.len();
        assert!(
            num_tiles > 0,
            "There must be at least one tile in the ruleset"
        );
        assert_eq!(
            frequencies.len(),
            adjacency_matrix.shape()[0],
            "Frequencies must match number of tiles"
        );
        assert_eq!(
            adjacency_matrix.shape(),
            &[num_tiles, num_tiles, 2],
            "Adjacency matrix must be shape [n, n, 2]"
        );

        let mut masks = Vec::with_capacity(num_tiles);
        for j in 0..num_tiles {
            let mut dirs = [
                FixedBitSet::with_capacity(num_tiles),
                FixedBitSet::with_capacity(num_tiles),
                FixedBitSet::with_capacity(num_tiles),
                FixedBitSet::with_capacity(num_tiles),
            ];
            for i in 0..num_tiles {
                if adjacency_matrix[[j, i, 1]] {
                    dirs[Direction::North.index()].insert(i);
                }
                if adjacency_matrix[[j, i, 0]] {
                    dirs[Direction::East.index()].insert(i);
                }
                if adjacency_matrix[[i, j, 1]] {
                    dirs[Direction::South.index()].insert(i);
                }
                if adjacency_matrix[[i, j, 0]] {
                    dirs[Direction::West.index()].insert(i);
                }
            }
            masks.push(dirs);
        }
        Rules { masks, frequencies }
    }

    pub fn len(&self) -> usize {
        self.masks.len()
    }

    pub fn masks(&self) -> &Vec<[FixedBitSet; 4]> {
        &self.masks
    }

    pub fn frequencies(&self) -> &[usize] {
        &self.frequencies
    }

    pub fn max_frequency(&self) -> Option<usize> {
        self.frequencies.iter().copied().max()
    }

    pub fn adjacency_matrix(&self) -> Array3<bool> {
        let num_tiles = self.len();
        let mut matrix = Array3::from_elem((num_tiles, num_tiles, 2), false);
        for i in 0..num_tiles {
            for j in 0..num_tiles {
                matrix[[i, j, 0]] = self.masks[i][Direction::East.index()].contains(j);
                matrix[[i, j, 1]] = self.masks[j][Direction::North.index()].contains(i);
            }
        }
        matrix
    }
}

impl Index<usize> for Rules {
    type Output = [FixedBitSet; 4];
    fn index(&self, idx: usize) -> &Self::Output {
        &self.masks[idx]
    }
}
