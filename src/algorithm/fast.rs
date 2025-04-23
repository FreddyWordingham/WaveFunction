use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::collections::{HashSet, VecDeque};

use crate::{Cell, Map, Rules, WaveFunction};

const MAX_ITERATIONS: usize = 1_000_000_000; // Max iterations for constraint propagation

// Precomputed direction deltas for faster access
const DIRECTION_DELTAS: [(isize, isize); 4] = [
    (1, 0),  // North
    (0, 1),  // East
    (-1, 0), // South
    (0, -1), // West
];

// Precomputed neighbour data structure that works with 2D coordinates
#[derive(Clone, Debug)]
struct Neighbour {
    pos: (usize, usize),
    dir: Direction,
    opp_dir: Direction,
}

pub struct WaveFunctionFast;

impl WaveFunction for WaveFunctionFast {
    /// Collapses a map using a hybrid optimized Wave Function Collapse algorithm
    /// Returns a new map with all wildcards collapsed to fixed values.
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map> {
        let (height, width) = map.size();
        let num_tiles = rules.len();

        // Use Array2 for domains and mask
        let mut domains = map.domains(num_tiles);
        let is_ignore = map.mask();

        // Pre-compute and cache domain sizes to avoid repeated counting
        let mut domain_sizes = Array2::from_elem((height, width), 0);

        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] {
                    let count = domains[(y, x)].count_ones(..);
                    domain_sizes[(y, x)] = count;
                }
            }
        }

        // Precompute neighbors for each cell for faster access
        let mut neighbors: Array2<Vec<Neighbour>> = Array2::from_elem((height, width), Vec::new());
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
                        neighbors[(y, x)].push(Neighbour {
                            pos: (ny, nx),
                            dir: *dir,
                            opp_dir: dir.opposite(),
                        });
                    }
                }
            }
        }

        // Optimized verify function that can check only affected cells
        fn verify_counts(
            domains: &Array2<FixedBitSet>,
            domain_sizes: &mut Array2<usize>,
            is_ignore: &Array2<bool>,
            affected_cells: Option<&HashSet<(usize, usize)>>,
        ) -> bool {
            let mut changed = false;

            // If affected cells are provided, only check those cells
            if let Some(cells) = affected_cells {
                for &(y, x) in cells {
                    if !is_ignore[(y, x)] {
                        let actual = domains[(y, x)].count_ones(..);
                        if domain_sizes[(y, x)] != actual {
                            domain_sizes[(y, x)] = actual;
                            changed = true;
                        }
                    }
                }
            } else {
                // Otherwise do a full check (should only be needed for initialization)
                let (height, width) = domains.dim();
                for y in 0..height {
                    for x in 0..width {
                        if !is_ignore[(y, x)] {
                            let actual = domains[(y, x)].count_ones(..);
                            if domain_sizes[(y, x)] != actual {
                                domain_sizes[(y, x)] = actual;
                                changed = true;
                            }
                        }
                    }
                }
            }

            changed
        }

        // Optimized revise function with fast paths
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

            // Standard case: check each value in xi domain
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

        // Set up constraint propagation queue
        let mut queue = VecDeque::with_capacity(4 * width * height);

        // Initial queue population with all constraints
        for y in 0..height {
            for x in 0..width {
                if is_ignore[(y, x)] {
                    continue;
                }

                for neighbor in &neighbors[(y, x)] {
                    queue.push_back(((y, x), neighbor.pos, neighbor.dir));
                }
            }
        }

        // Initial propagation - full AC-3
        let mut iteration_count = 0;

        while let Some((xi, xj, dir)) = queue.pop_front() {
            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                bail!("Too many constraint propagation iterations - possible infinite loop");
            }

            if revise(&mut domains, &mut domain_sizes, rules, xi, xj, dir) {
                if domain_sizes[xi] == 0 {
                    bail!("No valid tiles remain at cell ({}, {})", xi.0, xi.1);
                }

                // Add all affected neighbors to queue except xj
                for neighbor in &neighbors[xi] {
                    if neighbor.pos != xj {
                        queue.push_back((neighbor.pos, xi, neighbor.opp_dir));
                    }
                }
            }
        }

        // We only need to verify counts after initial propagation for safety
        // This could be removed for performance if domain_sizes tracking is solid
        let _ = verify_counts(&domains, &mut domain_sizes, &is_ignore, None);

        // Count cells to collapse for progress bar
        let mut cells_to_collapse = 0;
        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] && domain_sizes[(y, x)] > 1 {
                    cells_to_collapse += 1;
                }
            }
        }

        let pb = ProgressBar::new(cells_to_collapse as u64);
        pb.set_style(
            ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                .unwrap()
                .progress_chars("##-"),
        );

        // More robust bucket management using HashSet to track cells by entropy
        let mut bucket_sets: Vec<HashSet<(usize, usize)>> = vec![HashSet::new(); num_tiles + 1];

        // Initial population of entropy buckets
        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] && domain_sizes[(y, x)] > 1 {
                    bucket_sets[domain_sizes[(y, x)]].insert((y, x));
                }
            }
        }

        // Main collapse loop with bucketed entropy selection
        'outer: while let Some(entropy) = (2..=num_tiles).find(|&e| !bucket_sets[e].is_empty()) {
            // Extract a cell from the current entropy bucket
            let best_idx = *bucket_sets[entropy].iter().next().unwrap();
            bucket_sets[entropy].remove(&best_idx);

            // Safety check - verify count matches domain
            let actual_count = domains[best_idx].count_ones(..);
            if actual_count != domain_sizes[best_idx] {
                domain_sizes[best_idx] = actual_count;
                if actual_count != entropy {
                    // Our bucket assignment was wrong, put it in the right bucket
                    if domain_sizes[best_idx] > 1 {
                        bucket_sets[domain_sizes[best_idx]].insert(best_idx);
                    }
                    continue 'outer;
                }
            }

            // Sample weighted by frequency
            let options: Vec<usize> = domains[best_idx].ones().collect();
            if options.is_empty() {
                bail!(
                    "No options remain for cell at ({}, {}), but count was {}",
                    best_idx.0,
                    best_idx.1,
                    domain_sizes[best_idx]
                );
            }

            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();

            // Check if any weights are zero to avoid potential panic
            if weights.iter().any(|&w| w == 0) {
                // Handle zero weights case - use uniform distribution instead
                let choice = options[rng.random_range(0..options.len())];
                domains[best_idx].clear();
                domains[best_idx].insert(choice);
                domain_sizes[best_idx] = 1;
            } else {
                // Use weighted distribution
                let dist = WeightedIndex::new(&weights).unwrap();
                let choice = options[dist.sample(rng)];

                // Fix the chosen cell
                domains[best_idx].clear();
                domains[best_idx].insert(choice);
                domain_sizes[best_idx] = 1;
            }

            pb.inc(1);

            // Propagate from this collapse
            queue.clear();
            for neighbor in &neighbors[best_idx] {
                queue.push_back((neighbor.pos, best_idx, neighbor.opp_dir));
            }

            // Track which cells are affected by constraint propagation
            let mut affected_cells = HashSet::new();

            iteration_count = 0;
            while let Some((xi, xj, dir)) = queue.pop_front() {
                iteration_count += 1;
                if iteration_count > MAX_ITERATIONS {
                    bail!(
                        "Too many constraint propagation iterations after collapse - possible infinite loop"
                    );
                }

                if revise(&mut domains, &mut domain_sizes, rules, xi, xj, dir) {
                    if domain_sizes[xi] == 0 {
                        bail!(
                            "No valid tiles remain after collapse at ({}, {})",
                            xi.0,
                            xi.1
                        );
                    }

                    // Track that this cell was affected
                    affected_cells.insert(xi);

                    // Add all affected neighbors to queue except xj
                    for neighbor in &neighbors[xi] {
                        if neighbor.pos != xj {
                            queue.push_back((neighbor.pos, xi, neighbor.opp_dir));
                        }
                    }
                }
            }

            // Only verify counts for affected cells (more efficient)
            verify_counts(
                &domains,
                &mut domain_sizes,
                &is_ignore,
                Some(&affected_cells),
            );

            // Update buckets for all affected cells
            for &cell_idx in &affected_cells {
                // Remove from old bucket if we were tracking it
                for e in 2..=num_tiles {
                    bucket_sets[e].remove(&cell_idx);
                }

                // Add to new bucket if still has multiple options
                if domain_sizes[cell_idx] > 1 {
                    bucket_sets[domain_sizes[cell_idx]].insert(cell_idx);
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
                    let tile = match bits.next() {
                        Some(t) => t,
                        None => bail!("No possibilities for cell at ({}, {})", y, x),
                    };
                    result[(y, x)] = Cell::Fixed(tile);
                }
            }
        }

        Ok(result)
    }
}
