use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use std::collections::{HashSet, VecDeque};

use super::backtracking::BacktrackState;

// Precomputed neighbour data structure that works with 2D coordinates
#[derive(Clone, Debug)]
pub struct Neighbour {
    pub pos: (usize, usize),
    pub dir: Direction,
    pub opp_dir: Direction,
}

// Efficiently calculate neighborhood information for a grid
pub fn calculate_neighbours(
    height: usize,
    width: usize,
    is_ignore: &Array2<bool>,
) -> Array2<Vec<Neighbour>> {
    let mut neighbors: Array2<Vec<Neighbour>> = Array2::from_elem((height, width), Vec::new());
    let bounds = (height, width);

    for y in 0..height {
        for x in 0..width {
            if is_ignore[(y, x)] {
                continue;
            }

            for dir in ALL_DIRECTIONS.iter() {
                // Use the direction's apply_to method for safer coordinate calculation
                if let Some(neighbor_pos) = dir.apply_to((y, x), bounds) {
                    if !is_ignore[neighbor_pos] {
                        neighbors[(y, x)].push(Neighbour {
                            pos: neighbor_pos,
                            dir: *dir,
                            opp_dir: dir.opposite(),
                        });
                    }
                }
            }
        }
    }

    neighbors
}

// Optimized constraint revision function
pub fn revise(
    domains: &mut Array2<FixedBitSet>,
    domain_sizes: &mut Array2<usize>,
    rules: &crate::Rules,
    xi: (usize, usize),
    xj: (usize, usize),
    dir: Direction,
) -> bool {
    let mut modified = false;
    let dir_index = dir.index();

    // Early exit if domain is already a singleton
    if domain_sizes[xi] <= 1 {
        return false;
    }

    // Fast path: if we have only one option in xj, we can directly filter xi
    if domain_sizes[xj] == 1 {
        let v = domains[xj].ones().next().unwrap();

        // Track which options to remove (without modifying domain during iteration)
        let mut remove_count = 0;

        // Use a stack-allocated bitvector for small domains, fallback to Vec for larger
        if domains[xi].len() <= 128 {
            let mut to_remove = [false; 128];

            for u in domains[xi].ones() {
                if !rules.masks()[u][dir_index].contains(v) {
                    if u < 128 {
                        to_remove[u] = true;
                        remove_count += 1;
                    }
                }
            }

            if remove_count > 0 {
                for u in 0..domains[xi].len().min(128) {
                    if to_remove[u] && domains[xi].contains(u) {
                        domains[xi].remove(u);
                    }
                }
                domain_sizes[xi] -= remove_count;
                modified = true;
            }
        } else {
            // For larger domains, use the bitvector directly
            // Create a temporary bitvector for efficient operations
            let mut domain_copy = domains[xi].clone();

            // Efficiently remove values without support by iterating once
            for u in domains[xi].ones() {
                if !rules.masks()[u][dir_index].contains(v) {
                    domain_copy.set(u, false);
                    remove_count += 1;
                }
            }

            if remove_count > 0 {
                // Replace the original domain with our modified copy
                domains[xi] = domain_copy;
                domain_sizes[xi] -= remove_count;
                modified = true;
            }
        }

        return modified;
    }

    // Standard case: check each value in xi domain against possible supports in xj
    // Create a temporary bitvector for efficient operations
    let mut domain_copy = domains[xi].clone();
    let mut modified_count = 0;

    for u in domains[xi].ones() {
        let mask = &rules.masks()[u][dir_index];
        let mut has_support = false;

        // Use early exit loop for better performance
        for v in domains[xj].ones() {
            if mask.contains(v) {
                has_support = true;
                break;
            }
        }

        if !has_support {
            domain_copy.set(u, false);
            modified_count += 1;
        }
    }

    if modified_count > 0 {
        domains[xi] = domain_copy;
        domain_sizes[xi] -= modified_count;
        modified = true;
    }

    modified
}

// Propagate constraints from a starting cell
pub fn propagate_constraints(
    domains: &mut Array2<FixedBitSet>,
    domain_sizes: &mut Array2<usize>,
    rules: &crate::Rules,
    neighbors: &Array2<Vec<Neighbour>>,
    start_cell: (usize, usize),
    max_iterations: usize,
    mut backtrack_state: Option<&mut BacktrackState>,
) -> Result<HashSet<(usize, usize)>> {
    let mut queue = VecDeque::new();
    let mut affected_cells = HashSet::new();

    // Start with the neighbors of the given cell
    for neighbor in &neighbors[start_cell] {
        queue.push_back((neighbor.pos, start_cell, neighbor.opp_dir));
    }

    let mut iteration_count = 0;
    while let Some((xi, xj, dir)) = queue.pop_front() {
        // Before modifying a domain, save its state if tracking for backtracking
        if let Some(state) = &mut backtrack_state {
            if !state.changed_cells.contains(&xi) {
                state.changed_cells.insert(xi);
                state.domain_copies.insert(xi, domains[xi].clone());
                state.domain_size_copies.insert(xi, domain_sizes[xi]);
            }
        }

        iteration_count += 1;
        if iteration_count > max_iterations {
            bail!("Too many constraint propagation iterations");
        }

        if revise(domains, domain_sizes, rules, xi, xj, dir) {
            if domain_sizes[xi] == 0 {
                bail!("No valid tiles remain at cell ({}, {})", xi.0, xi.1);
            }

            // Track affected cells for domain bucket updates
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

// Perform initial constraint propagation on the entire grid
pub fn initial_propagation(
    domains: &mut Array2<FixedBitSet>,
    domain_sizes: &mut Array2<usize>,
    rules: &crate::Rules,
    height: usize,
    width: usize,
    is_ignore: &Array2<bool>,
    neighbors: &Array2<Vec<Neighbour>>,
    max_iterations: usize,
) -> Result<()> {
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
        if iteration_count > max_iterations {
            bail!("Too many initial constraint propagation iterations");
        }

        if revise(domains, domain_sizes, rules, xi, xj, dir) {
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

    Ok(())
}
