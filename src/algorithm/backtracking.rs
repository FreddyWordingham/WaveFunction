use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::{Cell, Map, Rules, WaveFunction};

const MAX_ITERATIONS: usize = 1_000_000; // Max iterations for constraint propagation
const MAX_BACKTRACK_ATTEMPTS: usize = 100; // Max number of backtracking attempts
const MAX_BACKTRACK_DEPTH: usize = 50; // Max depth for backtracking stack

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

// Structure to store state for backtracking
#[derive(Clone)]
struct BacktrackState {
    domains: Array2<FixedBitSet>,
    domain_sizes: Array2<usize>,
    cell: (usize, usize),
    tried_values: HashSet<usize>,
    collapsed_cells: HashSet<(usize, usize)>,
}

pub struct WaveFunctionBacktracking;

impl WaveFunction for WaveFunctionBacktracking {
    /// Collapses a map using a backtracking-capable Wave Function Collapse algorithm
    /// Returns a new map with all wildcards collapsed to fixed values.
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map> {
        let (height, width) = map.size();
        let num_tiles = rules.len();

        // Use Array2 for domains and mask
        let mut domains = map.domains(num_tiles);
        let is_ignore = map.mask();

        // Pre-compute and cache domain sizes
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

        // Function to revise constraints
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

            // Fast path: if we have only one option in xj, directly filter xi
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

        // Function to propagate constraints
        fn propagate_constraints(
            domains: &mut Array2<FixedBitSet>,
            domain_sizes: &mut Array2<usize>,
            rules: &Rules,
            neighbors: &Array2<Vec<Neighbour>>,
            start_cell: (usize, usize),
        ) -> Result<HashSet<(usize, usize)>> {
            let mut queue = VecDeque::new();
            let mut affected_cells = HashSet::new();

            // Initialize queue with starting cell's neighbors
            for neighbor in &neighbors[start_cell] {
                queue.push_back((neighbor.pos, start_cell, neighbor.opp_dir));
            }

            let mut iteration_count = 0;
            while let Some((xi, xj, dir)) = queue.pop_front() {
                iteration_count += 1;
                if iteration_count > MAX_ITERATIONS {
                    bail!("Too many constraint propagation iterations");
                }

                if revise(domains, domain_sizes, rules, xi, xj, dir) {
                    if domain_sizes[xi] == 0 {
                        bail!("No valid tiles remain at cell ({}, {})", xi.0, xi.1);
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

            Ok(affected_cells)
        }

        // Set up initial constraint propagation queue
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
                bail!("Too many initial constraint propagation iterations");
            }

            if revise(&mut domains, &mut domain_sizes, rules, xi, xj, dir) {
                if domain_sizes[xi] == 0 {
                    bail!(
                        "No valid tiles remain at cell ({}, {}) during initial propagation",
                        xi.0,
                        xi.1
                    );
                }

                // Add all affected neighbors to queue except xj
                for neighbor in &neighbors[xi] {
                    if neighbor.pos != xj {
                        queue.push_back((neighbor.pos, xi, neighbor.opp_dir));
                    }
                }
            }
        }

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
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} cells (Backtracked: {msg})"
            )
            .unwrap()
            .progress_chars("##-"),
        );
        pb.set_message("0");

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

        // Backtracking stack
        let mut backtrack_stack: Vec<BacktrackState> = Vec::with_capacity(MAX_BACKTRACK_DEPTH);
        let mut backtrack_count = 0;
        let mut collapsed_cells = HashSet::new();
        let start_time = Instant::now();

        // Main collapse loop with backtracking
        'outer: while let Some(entropy) = (2..=num_tiles).find(|&e| !bucket_sets[e].is_empty()) {
            // Extract a cell from the current entropy bucket
            let best_idx = *bucket_sets[entropy].iter().next().unwrap();
            bucket_sets[entropy].remove(&best_idx);

            // Get available options for this cell
            let options: Vec<usize> = domains[best_idx].ones().collect();
            if options.is_empty() {
                // This shouldn't happen normally, but handle it just in case
                if backtrack_stack.is_empty() {
                    bail!(
                        "No options remain for cell at ({}, {}), but backtrack stack is empty",
                        best_idx.0,
                        best_idx.1
                    );
                }

                continue; // Skip this cell and try the next one
            }

            // Calculate weights for weighted random selection
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();

            // Save state for backtracking
            let backtrack_state = BacktrackState {
                domains: domains.clone(),
                domain_sizes: domain_sizes.clone(),
                cell: best_idx,
                tried_values: HashSet::new(),
                collapsed_cells: collapsed_cells.clone(),
            };

            // If backtrack stack is too large, remove oldest entries
            while backtrack_stack.len() >= MAX_BACKTRACK_DEPTH {
                backtrack_stack.remove(0);
            }

            // Push current state to backtrack stack
            backtrack_stack.push(backtrack_state);

            // Choose a tile using weighted distribution
            let choice = if weights.iter().any(|&w| w == 0) {
                // Handle zero weights case - use uniform distribution
                options[rng.random_range(0..options.len())]
            } else {
                // Use weighted distribution
                let dist = WeightedIndex::new(&weights).unwrap();
                options[dist.sample(rng)]
            };

            // Fix the chosen cell
            domains[best_idx].clear();
            domains[best_idx].insert(choice);
            domain_sizes[best_idx] = 1;
            collapsed_cells.insert(best_idx);

            pb.inc(1);

            // Propagate constraints
            let propagation_result =
                propagate_constraints(&mut domains, &mut domain_sizes, rules, &neighbors, best_idx);

            match propagation_result {
                Ok(affected_cells) => {
                    // Update buckets for all affected cells
                    for &cell_idx in &affected_cells {
                        // Remove from old bucket
                        for e in 2..=num_tiles {
                            bucket_sets[e].remove(&cell_idx);
                        }

                        // Add to new bucket if still has multiple options
                        if domain_sizes[cell_idx] > 1 {
                            bucket_sets[domain_sizes[cell_idx]].insert(cell_idx);
                        }
                    }
                }
                Err(_) => {
                    // Constraint propagation failed
                    backtrack_count += 1;
                    pb.set_message(backtrack_count.to_string());

                    if backtrack_count > MAX_BACKTRACK_ATTEMPTS {
                        bail!("Maximum backtracking attempts exceeded");
                    }

                    // Pop the last state from the stack
                    if let Some(mut state) = backtrack_stack.pop() {
                        // Mark the choice we just tried as invalid
                        state.tried_values.insert(choice);

                        // Restore domains and other state
                        domains = state.domains.clone();
                        domain_sizes = state.domain_sizes.clone();
                        collapsed_cells = state.collapsed_cells.clone();

                        // Get remaining options that haven't been tried yet
                        let remaining_options: Vec<usize> = domains[state.cell]
                            .ones()
                            .filter(|&opt| !state.tried_values.contains(&opt))
                            .collect();

                        if remaining_options.is_empty() {
                            // No options left for this cell, need to backtrack further
                            continue 'outer;
                        }

                        // Choose a different option
                        let weights: Vec<usize> = remaining_options
                            .iter()
                            .map(|&t| rules.frequencies()[t])
                            .collect();

                        let choice = if weights.iter().any(|&w| w == 0) {
                            // Use uniform distribution
                            remaining_options[rng.random_range(0..remaining_options.len())]
                        } else {
                            // Use weighted distribution
                            let dist = WeightedIndex::new(&weights).unwrap();
                            remaining_options[dist.sample(rng)]
                        };

                        // Update the cell with new choice
                        domains[state.cell].clear();
                        domains[state.cell].insert(choice);
                        domain_sizes[state.cell] = 1;
                        collapsed_cells.insert(state.cell);

                        // Update state and push back to stack with the new tried value
                        state.tried_values.insert(choice);
                        backtrack_stack.push(state);

                        // Recalculate all buckets after backtracking
                        bucket_sets = vec![HashSet::new(); num_tiles + 1];
                        for y in 0..height {
                            for x in 0..width {
                                if !is_ignore[(y, x)] && domain_sizes[(y, x)] > 1 {
                                    bucket_sets[domain_sizes[(y, x)]].insert((y, x));
                                }
                            }
                        }
                    }
                }
            }

            // Periodically report progress and check timeout
            if start_time.elapsed() > Duration::from_secs(10) && backtrack_count > 0 {
                pb.println(format!(
                    "Progress: {}/{} cells, {} backtracks so far",
                    collapsed_cells.len(),
                    cells_to_collapse,
                    backtrack_count
                ));
            }
        }

        pb.finish_and_clear();

        // If we had to backtrack, report the final count
        if backtrack_count > 0 {
            println!("Completed with {} backtracking attempts", backtrack_count);
        }

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
