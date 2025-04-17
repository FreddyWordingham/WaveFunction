use anyhow::{Result, anyhow};
use bitvec::prelude::*;
use ndarray::Array2;
use photo::Direction;
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

const NEIGHBOURS: &[(isize, isize)] = &[(0, -1), (1, 0), (0, 1), (-1, 0)];

use crate::{Map, Tile, Tileset};

pub struct WaveFunction<'a> {
    possibilities: Array2<BitVec>,
    mask: Array2<bool>,
    tileset: &'a Tileset,
}

impl<'a> WaveFunction<'a> {
    pub fn new(map: &Map, tileset: &'a Tileset) -> Self {
        debug_assert!(map.max_index().is_some(), "Map must have a maximum index");
        debug_assert!(
            map.max_index().unwrap() < tileset.len(),
            "Map index out of bounds for tileset"
        );

        Self {
            possibilities: map.tiles().mapv(|tile| match tile {
                Tile::Fixed(n) => {
                    let mut p = bitvec![0; tileset.len()];
                    p.set(n, true);
                    p
                }
                _ => bitvec![1; tileset.len()],
            }),
            mask: map.tiles().mapv(|tile| match tile {
                Tile::Ignore => false,
                _ => true,
            }),
            tileset,
        }
    }

    /// Constraint propagation using AC-3.
    pub fn ac3(&mut self) -> Result<()> {
        let (width, height) = self.possibilities.dim();
        let mut queue = VecDeque::new();

        // Initialize the queue only for non-wildcard cells.
        for y in 0..height {
            for x in 0..width {
                // Skip arcs from or to wildcard cells.
                if !self.mask[(x, y)] {
                    continue;
                }
                for &(dy, dx) in NEIGHBOURS {
                    let ny = y as isize + dy;
                    let nx = x as isize + dx;
                    if ny >= 0 && nx >= 0 && ny < height as isize && nx < width as isize {
                        if !self.mask[(nx as usize, ny as usize)] {
                            continue;
                        }
                        queue.push_back(((y, x), ((ny as usize, nx as usize), (dy, dx))));
                    }
                }
            }
        }

        while let Some(((y, x), ((ny, nx), (dy, dx)))) = queue.pop_front() {
            if self.revise((y, x), (ny, nx), (dy, dx))? {
                if self.possibilities[(y, x)].not_any() {
                    return Err(anyhow!(
                        "AC-3 failed: cell ({},{}) has no valid possibilities.",
                        x,
                        y
                    ));
                }
                for &(ddy, ddx) in NEIGHBOURS {
                    let py = y as isize + ddy;
                    let px = x as isize + ddx;
                    if py >= 0 && px >= 0 && py < height as isize && px < width as isize {
                        if (py as usize, px as usize) == (ny, nx) {
                            continue;
                        }
                        if !self.mask[(px as usize, py as usize)] {
                            continue;
                        }
                        queue.push_back((
                            (py as usize, px as usize),
                            ((y, x), ((y as isize - py), (x as isize - px))),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Revise constraints between two cells.
    /// If either cell is a wildcard then no revision is necessary.
    fn revise(
        &mut self,
        pos: (usize, usize),
        n: (usize, usize),
        delta: (isize, isize),
    ) -> Result<bool> {
        let (y, x) = pos;
        let (ny, nx) = n;

        // Skip revision if either cell is a wildcard.
        if !self.mask[(x, y)] || !self.mask[(nx, ny)] {
            return Ok(false);
        }

        let mut revised = false;
        let num_tiles = self.tileset.len();
        let direction = match delta {
            (-1, 0) => Direction::North,
            (1, 0) => Direction::South,
            (0, 1) => Direction::East,
            (0, -1) => Direction::West,
            _ => panic!("Invalid direction"),
        };

        for tile in 0..num_tiles {
            if !self.possibilities[(y, x)][tile] {
                continue;
            }
            let allowed_mask: &BitVec = &self.tileset.rules()[tile][direction.index::<usize>()];
            let neighbor_possibilities = &self.possibilities[(ny, nx)];
            if !allowed_mask
                .iter()
                .zip(neighbor_possibilities.iter())
                .any(|(allowed, possible)| *allowed && *possible)
            {
                self.possibilities[(y, x)].set(tile, false);
                revised = true;
            }
        }
        Ok(revised)
    }

    /// A cell is considered collapsed if it either is a wildcard or has exactly one possibility.
    fn all_collapsed(&self) -> bool {
        self.possibilities
            .indexed_iter()
            .all(|((x, y), poss)| !self.mask[(x, y)] || poss.iter().filter(|b| **b).count() == 1)
    }

    /// Collapse the wave function.
    pub fn collapse<R: Rng>(&mut self, rng: &mut R) -> Result<Map> {
        use indicatif::{ProgressBar, ProgressStyle};
        let (width, height) = self.possibilities.dim();
        let total_cells = width * height;
        let progress_bar = ProgressBar::new(total_cells as u64);
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )?
                .progress_chars("#>-"),
        );

        let mut heap = BinaryHeap::new();

        // Only process non-wildcard cells.
        for ((x, y), poss) in self.possibilities.indexed_iter() {
            if !self.mask[(x, y)] {
                progress_bar.inc(1);
                continue;
            }
            let count = poss.iter().filter(|b| **b).count();
            if count > 1 {
                heap.push(Cell {
                    entropy: count,
                    x,
                    y,
                });
            } else {
                progress_bar.inc(1);
            }
        }

        while let Some(cell) = heap.pop() {
            let (x, y) = (cell.x, cell.y);
            let current_entropy = self.possibilities[(x, y)].iter().filter(|b| **b).count();

            if current_entropy == 0 {
                return Err(anyhow!("Cell ({},{}) has no possibilities", x, y));
            }
            if current_entropy == 1 {
                continue;
            }
            if current_entropy != cell.entropy {
                heap.push(Cell {
                    entropy: current_entropy,
                    x,
                    y,
                });
                continue;
            }

            let options: Vec<usize> = self.possibilities[(x, y)]
                .iter()
                .enumerate()
                .filter(|(_, possible)| **possible)
                .map(|(i, _)| i)
                .collect();
            let weights: Vec<usize> = options
                .iter()
                .map(|&tile| self.tileset.frequency(tile))
                .collect();
            let dist = WeightedIndex::new(&weights)
                .map_err(|e| anyhow!("Weighted distribution error: {}", e))?;
            let selected = options[dist.sample(rng)];

            for i in 0..self.possibilities[(x, y)].len() {
                self.possibilities[(x, y)].set(i, i == selected);
            }
            progress_bar.inc(1);
            self.ac3()?;

            // Update only non-wildcard neighbors.
            for &(dx, dy) in NEIGHBOURS {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx >= 0 && ny >= 0 && nx < width as isize && ny < height as isize {
                    let (nx, ny) = (nx as usize, ny as usize);
                    if !self.mask[(nx, ny)] {
                        continue;
                    }
                    let count = self.possibilities[(nx, ny)].iter().filter(|b| **b).count();
                    if count > 1 {
                        heap.push(Cell {
                            entropy: count,
                            x: nx,
                            y: ny,
                        });
                    }
                }
            }
        }
        progress_bar.finish();

        if !self.all_collapsed() {
            return Err(anyhow!("Not all tiles collapsed after optimization."));
        }

        let tiles = self
            .possibilities
            .indexed_iter()
            .map(|((x, y), poss)| {
                let selected = poss
                    .iter()
                    .enumerate()
                    .find(|(_, b)| **b)
                    .ok_or_else(|| anyhow!("No tile selected at ({}, {})", x, y))?
                    .0;
                Ok(selected)
            })
            .collect::<Result<Vec<_>>>()?;
        let tiles = Array2::from_shape_vec(
            (height, width),
            tiles.into_iter().map(|tile| Tile::Fixed(tile)).collect(),
        )
        .map_err(|e| anyhow!("Failed to create tiles array: {}", e))?;

        Ok(Map::new(tiles))
    }
}

/// Helper struct for the BinaryHeap.
#[derive(Copy, Clone, Eq, PartialEq)]
struct Cell {
    entropy: usize,
    x: usize,
    y: usize,
}

impl Ord for Cell {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .entropy
            .cmp(&self.entropy)
            .then_with(|| self.x.cmp(&other.x))
            .then_with(|| self.y.cmp(&other.y))
    }
}

impl PartialOrd for Cell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
