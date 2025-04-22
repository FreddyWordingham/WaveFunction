use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

use crate::{Cell, Map, Rules, WaveFunction};

// Precomputed direction deltas for faster access
const DIRECTION_DELTAS: [(isize, isize); 4] = [
    (-1, 0), // North
    (0, 1),  // East
    (1, 0),  // South
    (0, -1), // West
];

// Helper struct to track entropy and position for the priority queue
#[derive(PartialEq, Eq, Debug)]
struct EntropyCell {
    entropy: usize,
    position: (usize, usize),
}

impl PartialOrd for EntropyCell {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntropyCell {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.entropy.cmp(&other.entropy)
    }
}

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

        // Pre-compute and cache domain sizes to avoid repeated counting
        let mut domain_sizes = Array2::from_elem((height, width), 0);
        let mut uncollapsed_cells = 0;

        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] {
                    let count = domains[(y, x)].count_ones(..);
                    domain_sizes[(y, x)] = count;
                    if count > 1 {
                        uncollapsed_cells += 1;
                    }
                }
            }
        }

        // Helper: run AC³ on the current domains, starting from given cells
        let mut queue = VecDeque::with_capacity(4 * width * height);
        let enqueue_all = |queue: &mut VecDeque<((usize, usize), (usize, usize), Direction)>| {
            for y in 0..height {
                for x in 0..width {
                    if is_ignore[(y, x)] {
                        continue;
                    }
                    for (i, dir) in ALL_DIRECTIONS.iter().enumerate() {
                        let (dy, dx) = DIRECTION_DELTAS[i];
                        let ny = y.wrapping_add(dy as usize);
                        let nx = x.wrapping_add(dx as usize);
                        if ny < height && nx < width && !is_ignore[(ny, nx)] {
                            queue.push_back(((y, x), (ny, nx), *dir));
                        }
                    }
                }
            }
        };

        // Optimized revise function
        fn revise(
            domains: &mut Array2<FixedBitSet>,
            domain_sizes: &mut Array2<usize>,
            rules: &Rules,
            xi: (usize, usize),
            xj: (usize, usize),
            dir: Direction,
        ) -> bool {
            let mut modified = false;
            let dir_index = dir.index::<usize>();

            // Early exit if domain is already a singleton
            if domain_sizes[xi] <= 1 {
                return false;
            }

            // Fast path: if we have only one option in xj, we can directly filter xi
            if domain_sizes[xj] == 1 {
                let v = domains[xj].ones().next().unwrap();
                let mut to_remove = Vec::new();

                for u in domains[xi].ones() {
                    if !rules.masks()[u][dir_index].contains(v) {
                        to_remove.push(u);
                    }
                }

                if !to_remove.is_empty() {
                    let remove_count = to_remove.len();
                    for &u in &to_remove {
                        domains[xi].remove(u);
                    }
                    domain_sizes[xi] -= remove_count;
                    modified = true;
                }

                return modified;
            }

            // Standard case: check each value in domain_i
            let mut to_remove = Vec::new();
            for u in domains[xi].ones() {
                let mask = &rules.masks()[u][dir_index];
                let mut has_support = false;

                for v in domains[xj].ones() {
                    if mask.contains(v) {
                        has_support = true;
                        break;
                    }
                }

                if !has_support {
                    to_remove.push(u);
                }
            }

            if !to_remove.is_empty() {
                let remove_count = to_remove.len();
                for &u in &to_remove {
                    domains[xi].remove(u);
                }
                domain_sizes[xi] -= remove_count;
                modified = true;
            }

            modified
        }

        // Full AC3 propagation for initial setup
        enqueue_all(&mut queue);

        // Process the initial constraint queue
        while let Some((xi, xj, dir)) = queue.pop_front() {
            if revise(&mut domains, &mut domain_sizes, rules, xi, xj, dir) {
                if domain_sizes[xi] == 0 {
                    bail!("No valid tiles remain at cell ({}, {})", xi.0, xi.1);
                }

                // Propagate change to neighbors of xi (except xj)
                for (i, neighbor_dir) in ALL_DIRECTIONS.iter().enumerate() {
                    let (dy, dx) = DIRECTION_DELTAS[i];
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

        // Set up progress bar
        let pb = ProgressBar::new(uncollapsed_cells as u64);
        pb.set_style(
            ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                .unwrap()
                .progress_chars("##-"),
        );

        // Build a priority queue for selecting cells by entropy
        let mut entropy_queue = BinaryHeap::with_capacity(uncollapsed_cells);

        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] && domain_sizes[(y, x)] > 1 {
                    entropy_queue.push(Reverse(EntropyCell {
                        entropy: domain_sizes[(y, x)],
                        position: (y, x),
                    }));
                }
            }
        }

        // Main loop: pick a cell with minimum entropy, collapse it, re-propagate
        while let Some(Reverse(cell)) = entropy_queue.pop() {
            let best_idx = cell.position;

            // Skip if the domain has changed since this was added to the queue
            if domain_sizes[best_idx] != cell.entropy {
                // If the cell is already collapsed, skip it
                if domain_sizes[best_idx] <= 1 {
                    continue;
                }

                // Re-add with updated entropy
                entropy_queue.push(Reverse(EntropyCell {
                    entropy: domain_sizes[best_idx],
                    position: best_idx,
                }));
                continue;
            }

            // Pick one tile weighted by frequency
            let domain = &domains[best_idx];
            let options: Vec<usize> = domain.ones().collect();
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();
            let dist = WeightedIndex::new(&weights).unwrap();
            let choice = options[dist.sample(rng)];

            pb.inc(1);

            // Fix the chosen cell
            domains[best_idx].clear();
            domains[best_idx].insert(choice);
            domain_sizes[best_idx] = 1;

            // Propagate from this collapse
            queue.clear();
            for (i, dir) in ALL_DIRECTIONS.iter().enumerate() {
                let (dy, dx) = DIRECTION_DELTAS[i];
                let ny = best_idx.0.wrapping_add(dy as usize);
                let nx = best_idx.1.wrapping_add(dx as usize);

                if ny < height && nx < width {
                    let neighbor = (ny, nx);
                    if !is_ignore[neighbor] {
                        queue.push_back((neighbor, best_idx, dir.opposite()));
                    }
                }
            }

            // Process constraints
            while let Some((xi, xj, dir)) = queue.pop_front() {
                if revise(&mut domains, &mut domain_sizes, rules, xi, xj, dir) {
                    if domain_sizes[xi] == 0 {
                        bail!(
                            "No valid tiles remain after collapse at ({}, {})",
                            xi.0,
                            xi.1
                        );
                    }

                    // If domain became a singleton, update the heap with newly impacted cells
                    if domain_sizes[xi] == 1 {
                        pb.inc(1);
                    }

                    // Propagate to neighbors
                    for (i, neighbor_dir) in ALL_DIRECTIONS.iter().enumerate() {
                        let (dy, dx) = DIRECTION_DELTAS[i];
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
                    let tile = bits.next().unwrap();
                    result[(y, x)] = Cell::Fixed(tile);
                }
            }
        }

        Ok(result)
    }
}
