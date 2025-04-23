use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use std::collections::{HashSet, VecDeque};

// Precomputed direction deltas for faster access (using the North=(-1,0) convention)
pub const DIRECTION_DELTAS: [(isize, isize); 4] = [
    (1, 0),  // North
    (0, 1),  // East
    (-1, 0), // South
    (0, -1), // West
];

// Precomputed neighbour data structure that works with 2D coordinates
#[derive(Clone, Debug)]
pub struct Neighbour {
    pub pos: (usize, usize),
    pub dir: Direction,
    pub opp_dir: Direction,
}

// Efficiently calculate neighborhood information for a grid
pub fn calculate_neighbors(
    height: usize,
    width: usize,
    is_ignore: &Array2<bool>,
) -> Array2<Vec<Neighbour>> {
    let mut neighbors: Array2<Vec<Neighbour>> = Array2::from_elem((height, width), Vec::new());

    for y in 0..height {
        for x in 0..width {
            if is_ignore[(y, x)] {
                continue;
            }

            for (i, dir) in ALL_DIRECTIONS.iter().enumerate() {
                let (dy, dx) = DIRECTION_DELTAS[i];

                // Safe wrapping addition with bounds check
                let ny = match y.checked_add_signed(dy) {
                    Some(val) if val < height => val,
                    _ => continue,
                };

                let nx = match x.checked_add_signed(dx) {
                    Some(val) if val < width => val,
                    _ => continue,
                };

                if !is_ignore[(ny, nx)] {
                    neighbors[(y, x)].push(Neighbour {
                        pos: (ny, nx),
                        dir: *dir,
                        opp_dir: dir.opposite(),
                    });
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
    let dir_index = dir.index::<usize>();

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
            let mut to_remove = Vec::new();

            for u in domains[xi].ones() {
                if !rules.masks()[u][dir_index].contains(v) {
                    to_remove.push(u);
                    remove_count += 1;
                }
            }

            if remove_count > 0 {
                for &u in &to_remove {
                    domains[xi].remove(u);
                }
                domain_sizes[xi] -= remove_count;
                modified = true;
            }
        }

        return modified;
    }

    // Standard case: check each value in xi domain against possible supports in xj
    let mut to_remove = Vec::new();
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

// Propagate constraints from a starting cell
pub fn propagate_constraints(
    domains: &mut Array2<FixedBitSet>,
    domain_sizes: &mut Array2<usize>,
    rules: &crate::Rules,
    neighbors: &Array2<Vec<Neighbour>>,
    start_cell: (usize, usize),
    max_iterations: usize,
) -> Result<HashSet<(usize, usize)>> {
    let mut queue = VecDeque::new();
    let mut affected_cells = HashSet::new();

    // Start with the neighbors of the given cell
    for neighbor in &neighbors[start_cell] {
        queue.push_back((neighbor.pos, start_cell, neighbor.opp_dir));
    }

    let mut iteration_count = 0;
    while let Some((xi, xj, dir)) = queue.pop_front() {
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
