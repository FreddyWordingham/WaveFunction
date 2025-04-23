use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::collections::HashSet;

use super::common::{calculate_neighbours, initial_propagation, propagate_constraints};
use crate::{Cell, Map, Rules, WaveFunction};

const MAX_ITERATIONS: usize = 1_000_000; // Max iterations for constraint propagation

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

        // One-time calculation of domain sizes
        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] {
                    domain_sizes[(y, x)] = domains[(y, x)].count_ones(..);
                }
            }
        }

        // Precompute neighbors for faster access
        let neighbors = calculate_neighbours(height, width, &is_ignore);

        // Initial constraint propagation across the entire grid
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
            ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                .unwrap()
                .progress_chars("##-"),
        );

        // More efficient bucket management - fixed-size array of hashsets
        // Each bucket corresponds to an entropy level (number of possible states)
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

            // Quick verification that domain size is correct
            // Only verify when we've taken a cell from a bucket, not on every domain change
            if domain_sizes[best_idx] != domains[best_idx].count_ones(..) {
                domain_sizes[best_idx] = domains[best_idx].count_ones(..);
                if domain_sizes[best_idx] != entropy {
                    // Our bucket assignment was wrong, put it in the right bucket
                    if domain_sizes[best_idx] > 1 {
                        bucket_sets[domain_sizes[best_idx]].insert(best_idx);
                    }
                    continue 'outer;
                }
            }

            // Get options and their frequencies
            let options: Vec<usize> = domains[best_idx].ones().collect();
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();

            // Choose a tile based on frequency weights
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

            pb.inc(1);

            // Propagate constraints from the collapsed cell using shared function
            match propagate_constraints(
                &mut domains,
                &mut domain_sizes,
                rules,
                &neighbors,
                best_idx,
                MAX_ITERATIONS,
                None, // No backtracking for fast algorithm
            ) {
                Ok(affected_cells) => {
                    // Update buckets for all affected cells
                    for &cell_idx in &affected_cells {
                        // First remove from all buckets (faster than trying to track which bucket)
                        for e in 2..=num_tiles {
                            bucket_sets[e].remove(&cell_idx);
                        }

                        // Now add to correct bucket if the cell still has multiple options
                        if domain_sizes[cell_idx] > 1 {
                            bucket_sets[domain_sizes[cell_idx]].insert(cell_idx);
                        }
                    }
                }
                Err(e) => {
                    // Handle constraint propagation failure
                    bail!("Constraint propagation failed: {}", e);
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
