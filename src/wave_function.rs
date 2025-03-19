use anyhow::{Result, anyhow};
use ndarray::Array2;
use rand::{prelude::IteratorRandom, rng};
use std::collections::VecDeque;

use crate::RuleSet;

#[derive(Debug, Clone)]
pub struct WaveFunction {
    possibilities: Array2<Vec<bool>>,
    ruleset: RuleSet,
    resolution: (usize, usize),
}

impl WaveFunction {
    pub fn new(ruleset: &RuleSet, resolution: [usize; 2]) -> Self {
        let n_tiles = ruleset.num_tiles();
        let shape = (resolution[0], resolution[1]);
        let initial = vec![true; n_tiles];
        let possibilities = Array2::from_shape_fn(shape, |_| initial.clone());
        Self {
            possibilities,
            ruleset: ruleset.clone(),
            resolution: shape,
        }
    }

    /// Manually set the tile at (x, y) to a specific tile index.
    pub fn set_tile(&mut self, x: usize, y: usize, tile: usize) -> Result<()> {
        let n_tiles = self.ruleset.num_tiles();
        if tile >= n_tiles {
            return Err(anyhow!("Tile index {} is out of bounds", tile));
        }
        // Set the cell's possibilities: only the chosen tile is true.
        for t in 0..n_tiles {
            self.possibilities[(x, y)][t] = t == tile;
        }
        // Propagate the new constraint using AC-3.
        self.ac3()?;
        Ok(())
    }

    /// Standard AC-3 algorithm.
    pub fn ac3(&mut self) -> Result<()> {
        let (width, height) = self.resolution;
        let mut queue = VecDeque::new();

        // Initialise queue with all arcs: ((x, y), (nx, ny), direction from (x,y) to (nx,ny))
        for x in 0..width {
            for y in 0..height {
                for &(dx, dy) in &[(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && ny >= 0 && nx < width as isize && ny < height as isize {
                        queue.push_back(((x, y), ((nx as usize, ny as usize), (dx, dy))));
                    }
                }
            }
        }

        // Process the queue
        while let Some(((x, y), ((nx, ny), (dx, dy)))) = queue.pop_front() {
            if self.revise(x, y, nx, ny, dx, dy)? {
                // If domain becomes empty, the algorithm fails.
                let domain_count = self.possibilities[(x, y)].iter().filter(|&&b| b).count();
                if domain_count == 0 {
                    return Err(anyhow!(
                        "AC-3 failed: cell ({},{}) has no valid possibilities.",
                        x,
                        y
                    ));
                }
                // Enqueue all arcs (p -> (x, y)) for neighbours p of (x,y) except (nx, ny)
                for &(ddx, ddy) in &[(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let px = x as isize + ddx;
                    let py = y as isize + ddy;
                    if px >= 0 && py >= 0 && px < width as isize && py < height as isize {
                        if (px as usize, py as usize) == (nx, ny) {
                            continue;
                        }
                        // The direction from neighbour (px,py) to (x,y)
                        queue.push_back((
                            (px as usize, py as usize),
                            ((x, y), ((x as isize - px), (y as isize - py))),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Revise cell (x,y) with respect to its neighbour (nx, ny) in direction (dx,dy)
    /// Returns true if the domain of (x,y) is revised.
    fn revise(
        &mut self,
        x: usize,
        y: usize,
        nx: usize,
        ny: usize,
        dx: isize,
        dy: isize,
    ) -> Result<bool> {
        let mut revised = false;
        let n_tiles = self.ruleset.num_tiles();

        // Define allowed neighbour tiles for a given tile in (x,y)
        let allowed = |tile: usize| -> &Vec<usize> {
            match (dx, dy) {
                (0, -1) => &self.ruleset.rule(tile).north,
                (1, 0) => &self.ruleset.rule(tile).east,
                (0, 1) => &self.ruleset.rule(tile).south,
                (-1, 0) => &self.ruleset.rule(tile).west,
                _ => panic!("Invalid direction"),
            }
        };

        for v in 0..n_tiles {
            if !self.possibilities[(x, y)][v] {
                continue;
            }
            let mut valid = false;
            // Check if there's at least one value in the neighbour that is allowed for v.
            for w in 0..n_tiles {
                if self.possibilities[(nx, ny)][w] && allowed(v).contains(&w) {
                    valid = true;
                    break;
                }
            }
            // If no neighbour value supports v, remove v from the domain.
            if !valid {
                self.possibilities[(x, y)][v] = false;
                revised = true;
            }
        }
        Ok(revised)
    }

    /// Generate a collapsed map.
    /// Must be called after collapsing.
    pub fn generate_map(&self) -> Result<Array2<usize>> {
        let mut map = Array2::from_elem(self.resolution, 0usize);

        // For each cell, select the collapsed tile.
        for ((x, y), poss) in self.possibilities.indexed_iter() {
            // Gather all possible tile indices.
            let options: Vec<usize> = poss
                .iter()
                .enumerate()
                .filter(|&(_, &possible)| possible)
                .map(|(i, _)| i)
                .collect();
            if options.is_empty() {
                return Err(anyhow!("Cell ({},{}) has no possibilities", x, y));
            } else if options.len() > 1 {
                return Err(anyhow!(
                    "Cell ({},{}) has multiple possibilities: {:?}",
                    x,
                    y,
                    options
                ));
            } else {
                map[(x, y)] = options[0];
            }
        }
        Ok(map)
    }

    /// Iteratively collapse cells and propagate constraints.
    pub fn collapse(&mut self) -> Result<()> {
        let mut rng = rng();
        while !self.all_collapsed() {
            // Pick the non-collapsed cell with minimum entropy.
            let mut min_entropy = usize::MAX;
            let mut chosen_cell = None;
            for ((x, y), poss) in self.possibilities.indexed_iter() {
                let count = poss.iter().filter(|&&b| b).count();
                if count > 1 && count < min_entropy {
                    min_entropy = count;
                    chosen_cell = Some((x, y));
                }
            }
            let (x, y) = chosen_cell.ok_or_else(|| anyhow!("No cell to collapse found."))?;

            // Randomly choose one possibility for this cell.
            let options: Vec<usize> = self.possibilities[(x, y)]
                .iter()
                .enumerate()
                .filter(|&(_, &b)| b)
                .map(|(i, _)| i)
                .collect();
            let selected = options
                .into_iter()
                .choose(&mut rng)
                .ok_or_else(|| anyhow!("Cell ({},{}) has no possibilities", x, y))?;

            // Collapse: set only the selected possibility to true.
            for i in 0..self.possibilities[(x, y)].len() {
                self.possibilities[(x, y)][i] = i == selected;
            }
            // Propagate constraints after the collapse.
            self.ac3()?;
        }
        Ok(())
    }

    /// Check if every cell is fully collapsed.
    fn all_collapsed(&self) -> bool {
        self.possibilities
            .iter()
            .all(|poss| poss.iter().filter(|&&b| b).count() == 1)
    }
}
