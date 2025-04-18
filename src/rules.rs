use fixedbitset::FixedBitSet;
use ndarray::Array3;
use photo::Direction;
use std::ops::Index;

pub struct Rules {
    masks: Vec<[FixedBitSet; 4]>, // [N, E, S, W]
}

impl Rules {
    pub fn new(adj: Array3<bool>) -> Self {
        let n = adj.shape()[0];
        assert_eq!(adj.shape(), &[n, n, 2], "Adjacency must be n×n×2");

        let mut masks = Vec::with_capacity(n);
        for t in 0..n {
            let mut dirs = [
                FixedBitSet::with_capacity(n),
                FixedBitSet::with_capacity(n),
                FixedBitSet::with_capacity(n),
                FixedBitSet::with_capacity(n),
            ];
            for i in 0..n {
                if adj[[t, i, 1]] {
                    dirs[Direction::North.index::<usize>()].insert(i);
                }
                if adj[[t, i, 0]] {
                    dirs[Direction::East.index::<usize>()].insert(i);
                }
                if adj[[i, t, 1]] {
                    dirs[Direction::South.index::<usize>()].insert(i);
                }
                if adj[[i, t, 0]] {
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
        let n = self.len();
        let mut m = Array3::from_elem((n, n, 2), false);
        for i in 0..n {
            for j in 0..n {
                m[[i, j, 0]] = self.masks[i][Direction::East.index::<usize>()].contains(j);
                m[[i, j, 1]] = self.masks[j][Direction::North.index::<usize>()].contains(i);
            }
        }
        m
    }
}

impl Index<usize> for Rules {
    type Output = [FixedBitSet; 4];
    fn index(&self, idx: usize) -> &Self::Output {
        &self.masks[idx]
    }
}
