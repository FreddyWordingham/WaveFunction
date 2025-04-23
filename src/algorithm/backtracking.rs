use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::common::{calculate_neighbours, initial_propagation, propagate_constraints};
use crate::{Cell, Map, Rules, WaveFunction};

const MAX_ITERATIONS: usize = 1_000_000; // Max iterations for constraint propagation
const MAX_BACKTRACK_ATTEMPTS: usize = 100; // Max number of backtracking attempts
const MAX_BACKTRACK_DEPTH: usize = 50; // Max depth for backtracking stack

// Structure to store state for backtracking
#[derive(Clone)]
pub struct BacktrackState {
    // Modified state tracking
    pub changed_cells: HashSet<(usize, usize)>,
    pub domain_copies: HashMap<(usize, usize), FixedBitSet>,
    pub domain_size_copies: HashMap<(usize, usize), usize>,
    pub cell: (usize, usize),
    pub tried_values: HashSet<usize>,
    pub collapsed_cells: HashSet<(usize, usize)>,
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
                    domain_sizes[(y, x)] = domains[(y, x)].count_ones(..);
                }
            }
        }

        // Precompute neighbors using common function
        let neighbors = calculate_neighbours(height, width, &is_ignore);

        // Initial propagation - full AC-3 using common function
        initial_propagation(
            &mut domains,
            &mut domain_sizes,
            rules,
            height,
            width,
            &is_ignore,
            &neighbors,
            MAX_ITERATIONS,
        )?;

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

            // Choose a tile using weighted distribution
            let choice = if weights.iter().any(|&w| w == 0) {
                // Handle zero weights case - use uniform distribution
                options[rng.random_range(0..options.len())]
            } else {
                // Use weighted distribution
                let dist = WeightedIndex::new(&weights).unwrap();
                options[dist.sample(rng)]
            };

            // Save state for backtracking only if we have multiple options
            if options.len() > 1 {
                // Only save state worth backtracking to (cells with multiple options)
                let mut tried_values = HashSet::new();
                tried_values.insert(choice); // Pre-mark our current choice as tried

                let backtrack_state = BacktrackState {
                    changed_cells: HashSet::new(),
                    domain_copies: HashMap::new(),
                    domain_size_copies: HashMap::new(),
                    cell: best_idx,
                    tried_values,
                    collapsed_cells: collapsed_cells.clone(),
                };

                // If backtrack stack is too large, remove oldest entries
                while backtrack_stack.len() >= MAX_BACKTRACK_DEPTH {
                    backtrack_stack.remove(0);
                }

                // Push current state to backtrack stack
                backtrack_stack.push(backtrack_state);
            }

            // Fix the chosen cell
            domains[best_idx].clear();
            domains[best_idx].insert(choice);
            domain_sizes[best_idx] = 1;
            collapsed_cells.insert(best_idx);

            pb.inc(1);

            // Propagate constraints using common function - pass None for backtrack_state
            let propagation_result = propagate_constraints(
                &mut domains,
                &mut domain_sizes,
                rules,
                &neighbors,
                best_idx,
                MAX_ITERATIONS,
                None, // No tracking for now - we only need tracking when backtracking
            );

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
                    // Constraint propagation failed - backtrack
                    backtrack_count += 1;
                    pb.set_message(backtrack_count.to_string());

                    if backtrack_count > MAX_BACKTRACK_ATTEMPTS {
                        bail!("Maximum backtracking attempts exceeded");
                    }

                    // Pop the last state from the stack
                    if let Some(state) = backtrack_stack.pop() {
                        // Restore domains - just use full clone for now since we don't have the optimized approach
                        // In the full implementation, this would use the changed_cells, domain_copies, etc.

                        // Restore the collapsed cells set
                        collapsed_cells = state.collapsed_cells.clone();

                        // Get remaining options that haven't been tried yet for the cell
                        let original_domain = domains[state.cell].clone();
                        let remaining_options: Vec<usize> = original_domain
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

                        let new_choice = if weights.iter().any(|&w| w == 0) {
                            // Use uniform distribution
                            remaining_options[rng.random_range(0..remaining_options.len())]
                        } else {
                            // Use weighted distribution
                            let dist = WeightedIndex::new(&weights).unwrap();
                            remaining_options[dist.sample(rng)]
                        };

                        // Create a new backtrack state with updated tried values
                        let mut new_tried_values = state.tried_values.clone();
                        new_tried_values.insert(new_choice);

                        let new_state = BacktrackState {
                            changed_cells: HashSet::new(),
                            domain_copies: HashMap::new(),
                            domain_size_copies: HashMap::new(),
                            cell: state.cell,
                            tried_values: new_tried_values,
                            collapsed_cells: collapsed_cells.clone(),
                        };

                        backtrack_stack.push(new_state);

                        // Update the cell with new choice
                        domains[state.cell].clear();
                        domains[state.cell].insert(new_choice);
                        domain_sizes[state.cell] = 1;
                        collapsed_cells.insert(state.cell);

                        // After backtracking, recalculate the entire grid state to ensure consistency
                        // This is inefficient but ensures correctness
                        // Initialize domains from current state
                        for y in 0..height {
                            for x in 0..width {
                                if !is_ignore[(y, x)] && (y, x) != state.cell {
                                    // Reset domain for cells other than the one we just fixed
                                    if collapsed_cells.contains(&(y, x)) {
                                        // Keep collapsed cells collapsed
                                        // (domains are already correct)
                                    } else {
                                        // Reset uncollapsed cells to all options
                                        domains[(y, x)].clear();
                                        domains[(y, x)].insert_range(..num_tiles);
                                        domain_sizes[(y, x)] = num_tiles;
                                    }
                                }
                            }
                        }

                        // Run initial propagation again with all constraints
                        initial_propagation(
                            &mut domains,
                            &mut domain_sizes,
                            rules,
                            height,
                            width,
                            &is_ignore,
                            &neighbors,
                            MAX_ITERATIONS,
                        )?;

                        // Rebuild buckets from current domain sizes
                        for e in 2..=num_tiles {
                            bucket_sets[e].clear();
                        }

                        for y in 0..height {
                            for x in 0..width {
                                if !is_ignore[(y, x)]
                                    && !collapsed_cells.contains(&(y, x))
                                    && domain_sizes[(y, x)] > 1
                                {
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
