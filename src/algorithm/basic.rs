use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::ALL_DIRECTIONS;
use photo::Direction;
use photo::Direction::*;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use std::collections::VecDeque;

use crate::{Cell, Map, Rules, WaveFunction};

pub struct WaveFunctionBasic;

impl WaveFunction for WaveFunctionBasic {
    /// Collapses a map using the Wave Function Collapse algorithm
    /// Returns a new map with all wildcards collapsed to fixed values.
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map> {
        let (height, width) = map.size();
        let num_tiles = rules.len();

        // Use Array2 for domains and is_ignore instead of flattened vectors
        let mut domains = map.domains(num_tiles);
        let is_ignore = map.mask();

        // Map directions to delta coordinates
        fn delta_for_direction(dir: Direction) -> (isize, isize) {
            match dir {
                North => (-1, 0),
                East => (0, 1),
                South => (1, 0),
                West => (0, -1),
            }
        }

        // Helper: run AC³ on the current domains, starting from `queue`
        let mut queue = VecDeque::new();
        let mut enqueue_all = || {
            for y in 0..height {
                for x in 0..width {
                    if is_ignore[(y, x)] {
                        continue;
                    }
                    for dir in ALL_DIRECTIONS.iter() {
                        let (dy, dx) = delta_for_direction(*dir);
                        let ny = y.wrapping_add(dy as usize);
                        let nx = x.wrapping_add(dx as usize);
                        if ny < height && nx < width && !is_ignore[(ny, nx)] {
                            queue.push_back(((y, x), (ny, nx), *dir));
                        }
                    }
                }
            }
        };

        fn revise(
            domains: &mut Array2<FixedBitSet>,
            rules: &Rules,
            xi: (usize, usize),
            xj: (usize, usize),
            dir: Direction,
        ) -> bool {
            let mut removed = Vec::new();
            for u in domains[xi].ones() {
                let mut ok = false;
                for v in domains[xj].ones() {
                    if rules.masks()[u][dir.index::<usize>()].contains(v) {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    removed.push(u);
                }
            }
            if removed.is_empty() {
                false
            } else {
                for u in removed {
                    domains[xi].remove(u);
                }
                true
            }
        }

        // Full AC3 propagation
        enqueue_all();
        while let Some((xi, xj, dir)) = queue.pop_front() {
            if revise(&mut domains, rules, xi, xj, dir) {
                if domains[xi].is_empty() {
                    bail!("No valid tiles remain at cell ({}, {})", xi.0, xi.1);
                }
                // propagate change to neighbors of xi (except xj)
                for neighbor_dir in ALL_DIRECTIONS.iter() {
                    let (dy, dx) = delta_for_direction(*neighbor_dir);
                    let ny = xi.0.wrapping_add(dy as usize);
                    let nx = xi.1.wrapping_add(dx as usize);
                    if ny < height && nx < width {
                        let xk = (ny, nx);
                        if xk != xj && !is_ignore[xk] {
                            queue.push_back((xk, xi, neighbor_dir.opposite()));
                        }
                    }
                }
            }
        }

        // Count how many cells to collapse
        let mut total = 0;
        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] && domains[(y, x)].count_ones(..) > 1 {
                    total += 1;
                }
            }
        }

        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                .unwrap()
                .progress_chars("##-"),
        );

        // Main loop: pick a cell with >1 possibility, collapse it, re-propagate
        loop {
            // Find the cell with the minimum number of possibilities
            let mut best_idx = None;
            let mut min_count = usize::MAX;

            for y in 0..height {
                for x in 0..width {
                    if !is_ignore[(y, x)] && domains[(y, x)].count_ones(..) > 1 {
                        let count = domains[(y, x)].count_ones(..);
                        if count < min_count {
                            min_count = count;
                            best_idx = Some((y, x));
                        }
                    }
                }
            }

            // If no more cells to collapse, we're done
            let best_idx = match best_idx {
                Some(idx) => idx,
                None => break,
            };

            // Pick one tile weighted by frequency
            let options: Vec<usize> = domains[best_idx].ones().collect();
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();
            let dist = WeightedIndex::new(&weights).unwrap();
            let choice = options[dist.sample(rng)];

            pb.inc(1);

            // Fix it
            domains[best_idx].clear();
            domains[best_idx].insert(choice);

            // Propagate from this collapse
            for dir in ALL_DIRECTIONS.iter() {
                let (dy, dx) = delta_for_direction(*dir);
                let ny = best_idx.0.wrapping_add(dy as usize);
                let nx = best_idx.1.wrapping_add(dx as usize);
                if ny < height && nx < width {
                    let neighbor = (ny, nx);
                    if !is_ignore[neighbor] {
                        queue.push_back((neighbor, best_idx, dir.opposite()));
                    }
                }
            }

            while let Some((xi, xj, dir)) = queue.pop_front() {
                if revise(&mut domains, rules, xi, xj, dir) {
                    if domains[xi].is_empty() {
                        bail!(
                            "No valid tiles remain after collapse at ({}, {})",
                            xi.0,
                            xi.1
                        );
                    }
                    for neighbor_dir in ALL_DIRECTIONS.iter() {
                        let (dy, dx) = delta_for_direction(*neighbor_dir);
                        let ny = xi.0.wrapping_add(dy as usize);
                        let nx = xi.1.wrapping_add(dx as usize);
                        if ny < height && nx < width {
                            let xk = (ny, nx);
                            if xk != xj && !is_ignore[xk] {
                                queue.push_back((xk, xi, neighbor_dir.opposite()));
                            }
                        }
                    }
                }
            }
        }
        pb.finish_and_clear();

        // Build the final map
        let mut result = map.clone();
        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] {
                    let mut bits = domains[(y, x)].ones();
                    let tile = bits.next().unwrap(); // <-- pull the single value
                    result[(y, x)] = Cell::Fixed(tile);
                }
            }
        }
        Ok(result)
    }
}
