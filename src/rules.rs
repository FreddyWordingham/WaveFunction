use fixedbitset::FixedBitSet;
use ndarray::Array3;
use photo::Direction;
use std::ops::Index;

pub struct Rules {
    masks: Vec<[FixedBitSet; 4]>, // [N, E, S, W]
}

impl Rules {
    pub fn new(adjacency_matrix: Array3<bool>) -> Self {
        let num_tiles = adjacency_matrix.shape()[0];
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
                    dirs[Direction::North.index::<usize>()].insert(i);
                }
                if adjacency_matrix[[j, i, 0]] {
                    dirs[Direction::East.index::<usize>()].insert(i);
                }
                if adjacency_matrix[[i, j, 1]] {
                    dirs[Direction::South.index::<usize>()].insert(i);
                }
                if adjacency_matrix[[i, j, 0]] {
                    dirs[Direction::West.index::<usize>()].insert(i);
                }
            }
            masks.push(dirs);
        }
        Rules { masks }
    }

    pub fn len(&self) -> usize {
        self.masks.len()
    }

    pub fn masks(&self) -> &Vec<[FixedBitSet; 4]> {
        &self.masks
    }

    pub fn adjacency_matrix(&self) -> Array3<bool> {
        let num_tiles = self.len();
        let mut matrix = Array3::from_elem((num_tiles, num_tiles, 2), false);
        for i in 0..num_tiles {
            for j in 0..num_tiles {
                matrix[[i, j, 0]] = self.masks[i][Direction::East.index::<usize>()].contains(j);
                matrix[[i, j, 1]] = self.masks[j][Direction::North.index::<usize>()].contains(i);
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
