use bitvec::prelude::*;
use ndarray::{Array3, s};
use photo::Direction;
use std::ops::Index;

pub struct Rules {
    adjacencies: Vec<[BitVec; 4]>,
}

impl Rules {
    pub fn new(adjacency_matrix: Array3<bool>) -> Self {
        debug_assert!(
            !adjacency_matrix.is_empty(),
            "Adjacency matrix must not be empty"
        );
        debug_assert_eq!(
            adjacency_matrix.shape()[0],
            adjacency_matrix.shape()[1],
            "Adjacency matrix must be square"
        );
        debug_assert_eq!(
            adjacency_matrix.shape()[2],
            2,
            "Adjacency matrix must have a third dimension of size 2"
        );

        let num_tiles = adjacency_matrix.shape()[0];
        let adjacencies = (0..num_tiles)
            .map(|tile| {
                let north: BitVec = adjacency_matrix
                    .slice(s![tile, .., 1])
                    .iter()
                    .cloned()
                    .collect();
                let east: BitVec = adjacency_matrix
                    .slice(s![tile, .., 0])
                    .iter()
                    .cloned()
                    .collect();
                let south: BitVec = adjacency_matrix
                    .slice(s![.., tile, 1])
                    .iter()
                    .cloned()
                    .collect();
                let west: BitVec = adjacency_matrix
                    .slice(s![.., tile, 0])
                    .iter()
                    .cloned()
                    .collect();
                [north, east, south, west]
            })
            .collect();

        Self { adjacencies }
    }

    pub fn len(&self) -> usize {
        self.adjacencies.len()
    }

    pub fn adjacency_matrix(&self) -> Array3<bool> {
        let num_tiles = self.len();
        let mut adjacency_matrix = Array3::from_elem((num_tiles, num_tiles, 2), false);
        for i in 0..num_tiles {
            for j in 0..num_tiles {
                let north_index = Direction::North.index::<usize>();
                let east_index = Direction::East.index::<usize>();
                adjacency_matrix[[i, j, 0]] = self.adjacencies[i][east_index][j];
                adjacency_matrix[[i, j, 1]] = self.adjacencies[j][north_index][i];
            }
        }
        adjacency_matrix
    }
}

impl Index<usize> for Rules {
    type Output = [BitVec; 4];

    fn index(&self, index: usize) -> &Self::Output {
        &self.adjacencies[index]
    }
}
