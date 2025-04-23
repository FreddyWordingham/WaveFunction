use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::common::{calculate_neighbors, initial_propagation, propagate_constraints};
use crate::{Cell, Map, Rules, WaveFunction};

const MAX_ITERATIONS: usize = 1_000_000; // Max iterations for constraint propagation
const MAX_BACKTRACK_ATTEMPTS: usize = 100; // Max number of backtracking attempts
const MAX_BACKTRACK_DEPTH: usize = 50; // Max depth for backtracking stack

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
                    domain_sizes[(y, x)] = domains[(y, x)].count_ones(..);
                }
            }
        }

        // Precompute neighbors using common function
        let neighbors = calculate_neighbors(height, width, &is_ignore);

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

            // Propagate constraints using common function
            let propagation_result = propagate_constraints(
                &mut domains,
                &mut domain_sizes,
                rules,
                &neighbors,
                best_idx,
                MAX_ITERATIONS,
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
