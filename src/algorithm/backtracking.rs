use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use photo::{ALL_DIRECTIONS, Direction};
use rand::{distr::weighted::WeightedIndex, prelude::*};
use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};

use crate::{Cell, Map, Rules, WaveFunction};

// Mapping from Direction to coordinate delta
fn delta_from_direction(dir: Direction) -> (isize, isize) {
    match dir {
        Direction::North => (-1, 0),
        Direction::East => (0, 1),
        Direction::South => (1, 0),
        Direction::West => (0, -1),
    }
}

// Precomputed neighbour data structure
#[derive(Clone)]
struct Neighbour {
    idx: usize,
    dir: Direction,
    opp_dir: Direction,
}

// Structure to represent a decision point for backtracking
#[derive(Clone)]
struct DecisionPoint {
    // Cell index where the decision was made
    cell_idx: usize,
    // The option chosen at this decision point
    choice: usize,
    // Options that have been tried and failed at this point
    tried_options: HashSet<usize>,
    // Complete state snapshot for backtracking
    domains: Vec<FixedBitSet>,
    counts: Vec<usize>,
    bucket_sets: Vec<HashSet<usize>>,
}

pub struct WaveFunctionWithBacktracking;

impl WaveFunction for WaveFunctionWithBacktracking {
    /// Collapses a map using the optimized Wave Function Collapse algorithm with backtracking
    /// Returns a new map with all wildcards collapsed to fixed values.
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map> {
        let (height, width) = map.size();
        let num_tiles = rules.len();
        let size = height * width;

        // Flattened domains; ignore cells get an empty bitset but are skipped below
        let mut domains: Vec<FixedBitSet> = Vec::with_capacity(size);
        let mut is_ignore = vec![false; size];

        // Cached counts for faster entropy calculations
        let mut counts = vec![0; size];

        // Initialize domains and counts
        for idx in 0..size {
            let r = idx / width;
            let c = idx % width;
            match map[(r, c)] {
                Cell::Ignore => {
                    let bs = FixedBitSet::with_capacity(num_tiles);
                    domains.push(bs);
                    is_ignore[idx] = true;
                    counts[idx] = 0;
                }
                Cell::Wildcard => {
                    let mut bs = FixedBitSet::with_capacity(num_tiles);
                    bs.insert_range(..num_tiles);
                    domains.push(bs);
                    counts[idx] = num_tiles;
                }
                Cell::Fixed(i) => {
                    let mut bs = FixedBitSet::with_capacity(num_tiles);
                    bs.insert(i);
                    domains.push(bs);
                    counts[idx] = 1;
                }
            }
        }

        // Precompute neighbours for faster access
        let mut neighbours: Vec<Vec<Neighbour>> = Vec::with_capacity(size);
        for idx in 0..size {
            let r = idx / width;
            let c = idx % width;
            let mut cell_neighbours = Vec::new();

            for dir in ALL_DIRECTIONS.iter() {
                let (dr, dc) = delta_from_direction(*dir);
                let nr = r.wrapping_add(dr as usize);
                let nc = c.wrapping_add(dc as usize);
                if nr < height && nc < width {
                    let neighbour_idx = nr * width + nc;
                    if !is_ignore[neighbour_idx] {
                        let opp_dir = dir.opposite();
                        cell_neighbours.push(Neighbour {
                            idx: neighbour_idx,
                            dir: *dir,
                            opp_dir,
                        });
                    }
                }
            }

            neighbours.push(cell_neighbours);
        }

        // Helper: run AC³ on the current domains, using an efficient queue
        let mut queue = VecDeque::new();

        // Initial queue population with all constraints
        for xi in 0..size {
            if is_ignore[xi] {
                continue;
            }

            for neighbour in &neighbours[xi] {
                queue.push_back((xi, neighbour.idx, neighbour.dir));
            }
        }

        // Verify and sync domain counts
        fn verify_counts(domains: &[FixedBitSet], counts: &mut [usize]) -> bool {
            let mut changed = false;
            for (i, domain) in domains.iter().enumerate() {
                let actual = domain.count_ones(..);
                if counts[i] != actual {
                    counts[i] = actual;
                    changed = true;
                }
            }
            changed
        }

        // Revise function that updates counts directly
        fn revise(
            domains: &mut [FixedBitSet],
            counts: &mut [usize],
            rules: &Rules,
            xi: usize,
            xj: usize,
            dir: Direction,
        ) -> bool {
            let d_idx = dir.index::<usize>();
            let mut changed = false;
            let current_domain = domains[xi].clone(); // Take a snapshot to iterate over

            for u in current_domain.ones() {
                let mut ok = false;
                for v in domains[xj].ones() {
                    if rules.masks()[u][d_idx].contains(v) {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    domains[xi].remove(u);
                    counts[xi] -= 1;
                    changed = true;
                }
            }

            changed
        }

        // Initial propagation - full AC-3
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 1_000_000; // Prevent infinite loops
        const MAX_BACKTRACK_ATTEMPTS: usize = 1000; // Limit backtracking attempts
        const MAX_RUNTIME_SECONDS: u64 = 300; // Time limit in seconds (5 minutes)

        let start_time = Instant::now();
        let mut backtrack_attempts = 0;

        // Initial propagation
        while let Some((xi, xj, dir)) = queue.pop_front() {
            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                bail!("Too many constraint propagation iterations - possible infinite loop");
            }

            if revise(&mut domains, &mut counts, rules, xi, xj, dir) {
                if counts[xi] == 0 {
                    // Initial constraints are unsatisfiable - no backtracking can help here
                    bail!(
                        "Initial constraints are unsatisfiable at cell ({}, {})",
                        xi / width,
                        xi % width
                    );
                }

                // Add all affected neighbours to queue except xj
                for neighbour in &neighbours[xi] {
                    if neighbour.idx != xj {
                        queue.push_back((neighbour.idx, xi, neighbour.opp_dir));
                    }
                }
            }
        }

        // Verify counts match domains after initial propagation
        verify_counts(&domains, &mut counts);

        // Count cells to collapse for progress bar - this counts only non-ignore cells with domains > 1
        let mut cells_to_collapse = 0;
        for i in 0..size {
            if !is_ignore[i] && counts[i] > 1 {
                cells_to_collapse += 1;
            }
        }

        let pb = ProgressBar::new(cells_to_collapse as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "{bar:40.cyan/blue} {pos}/{len} cells [{elapsed_precise}]",
            )
            .unwrap()
            .progress_chars("##-"),
        );

        // More robust bucket management using HashSet to track cells in each bucket
        let mut bucket_sets: Vec<HashSet<usize>> = vec![HashSet::new(); num_tiles + 1];

        // Initial population of entropy buckets
        for i in 0..size {
            if !is_ignore[i] && counts[i] > 1 {
                bucket_sets[counts[i]].insert(i);
            }
        }

        // Stack for backtracking
        let mut decision_stack: Vec<DecisionPoint> = Vec::new();

        // Main collapse loop with bucketed entropy selection and backtracking
        'outer: loop {
            // Check if we're done - no more cells to collapse
            if (2..=num_tiles).all(|e| bucket_sets[e].is_empty()) {
                break 'outer; // All cells collapsed successfully
            }

            // Check time constraints
            if start_time.elapsed() > Duration::from_secs(MAX_RUNTIME_SECONDS) {
                bail!("Time limit exceeded ({} seconds)", MAX_RUNTIME_SECONDS);
            }

            // Find the lowest entropy cell
            let entropy = match (2..=num_tiles).find(|&e| !bucket_sets[e].is_empty()) {
                Some(e) => e,
                None => {
                    // No cells with multiple options - we're either done or something went wrong
                    if decision_stack.is_empty() {
                        break 'outer; // All cells collapsed successfully
                    } else {
                        // This shouldn't happen in normal operation
                        bail!("No cells with multiple options, but still have decision points");
                    }
                }
            };

            // Extract a cell from the current entropy bucket
            let best_idx = *bucket_sets[entropy].iter().next().unwrap();
            bucket_sets[entropy].remove(&best_idx);

            // Safety check - verify count matches domain
            let actual_count = domains[best_idx].count_ones(..);
            if actual_count != counts[best_idx] {
                counts[best_idx] = actual_count;
                if actual_count != entropy {
                    // Our bucket assignment was wrong, put it in the right bucket
                    if counts[best_idx] > 1 {
                        bucket_sets[counts[best_idx]].insert(best_idx);
                    }
                    continue 'outer;
                }
            }

            // Get all options for this cell
            let options: Vec<usize> = domains[best_idx].ones().collect();
            if options.is_empty() {
                // This shouldn't happen due to earlier checks, but just in case
                bail!(
                    "No options remain for cell at {}, but count was {}",
                    best_idx,
                    counts[best_idx]
                );
            }

            // Sample weighted by frequency
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();
            let dist = WeightedIndex::new(&weights).unwrap();
            let choice = options[dist.sample(rng)];

            // Save current state before making a decision
            let decision_point = DecisionPoint {
                cell_idx: best_idx,
                choice,
                tried_options: HashSet::from([choice]),
                domains: domains.clone(),
                counts: counts.clone(),
                bucket_sets: bucket_sets.clone(),
            };

            decision_stack.push(decision_point);

            // Make the choice
            domains[best_idx].clear();
            domains[best_idx].insert(choice);
            counts[best_idx] = 1;

            pb.inc(1);

            let mut inconsistency_found = false;

            // Propagate from this collapse - full AC-3
            queue.clear();
            for neighbour in &neighbours[best_idx] {
                queue.push_back((neighbour.idx, best_idx, neighbour.opp_dir));
            }

            // Track which cells are affected by constraint propagation to update buckets
            let mut affected_cells = HashSet::new();

            iteration_count = 0;
            while let Some((xi, xj, dir)) = queue.pop_front() {
                iteration_count += 1;
                if iteration_count > MAX_ITERATIONS {
                    bail!(
                        "Too many constraint propagation iterations after collapse - possible infinite loop"
                    );
                }

                if revise(&mut domains, &mut counts, rules, xi, xj, dir) {
                    if counts[xi] == 0 {
                        // Inconsistency detected - need to backtrack
                        inconsistency_found = true;
                        break;
                    }

                    // Track that this cell was affected
                    affected_cells.insert(xi);

                    // Add all affected neighbours to queue except xj
                    for neighbour in &neighbours[xi] {
                        if neighbour.idx != xj {
                            queue.push_back((neighbour.idx, xi, neighbour.opp_dir));
                        }
                    }
                }
            }

            if inconsistency_found {
                // Need to backtrack
                backtrack_attempts += 1;

                if backtrack_attempts > MAX_BACKTRACK_ATTEMPTS {
                    bail!(
                        "Maximum backtrack attempts ({}) exceeded",
                        MAX_BACKTRACK_ATTEMPTS
                    );
                }

                // Backtracking loop
                'backtrack: loop {
                    if decision_stack.is_empty() {
                        bail!("Backtracking exhausted all possibilities - no solution exists");
                    }

                    // Get the last decision point
                    let mut last_decision = decision_stack.pop().unwrap();

                    // Find untried options at this decision point
                    let all_options: Vec<usize> = last_decision.domains[last_decision.cell_idx]
                        .ones()
                        .collect();

                    let untried_options: Vec<usize> = all_options
                        .into_iter()
                        .filter(|&opt| !last_decision.tried_options.contains(&opt))
                        .collect();

                    if untried_options.is_empty() {
                        // All options tried at this level, backtrack further
                        continue 'backtrack;
                    }

                    // Pick a new option
                    // We could use weighted selection here too
                    let new_choice = untried_options[rng.random_range(0..untried_options.len())];

                    // Add this option to tried options
                    last_decision.tried_options.insert(new_choice);

                    // Restore state from this decision point
                    domains = last_decision.domains.clone();
                    counts = last_decision.counts.clone();
                    bucket_sets = last_decision.bucket_sets.clone();

                    // Apply the new choice
                    domains[last_decision.cell_idx].clear();
                    domains[last_decision.cell_idx].insert(new_choice);
                    counts[last_decision.cell_idx] = 1;

                    // Update decision with the new choice
                    last_decision.choice = new_choice;

                    // Store the cell_idx before we move last_decision
                    let cell_idx = last_decision.cell_idx;

                    // Push updated decision back to stack
                    decision_stack.push(last_decision);

                    // Propagate from this new choice
                    queue.clear();
                    for neighbour in &neighbours[cell_idx] {
                        queue.push_back((neighbour.idx, cell_idx, neighbour.opp_dir));
                    }

                    // Check if this new choice leads to inconsistency
                    inconsistency_found = false;
                    affected_cells.clear();

                    iteration_count = 0;
                    while let Some((xi, xj, dir)) = queue.pop_front() {
                        iteration_count += 1;
                        if iteration_count > MAX_ITERATIONS {
                            bail!("Too many iterations during backtracking");
                        }

                        if revise(&mut domains, &mut counts, rules, xi, xj, dir) {
                            if counts[xi] == 0 {
                                // Still inconsistent with new choice
                                inconsistency_found = true;
                                break;
                            }

                            affected_cells.insert(xi);

                            for neighbour in &neighbours[xi] {
                                if neighbour.idx != xj {
                                    queue.push_back((neighbour.idx, xi, neighbour.opp_dir));
                                }
                            }
                        }
                    }

                    if !inconsistency_found {
                        // This choice works - update buckets and continue
                        for &cell_idx in &affected_cells {
                            // Remove from old bucket if we were tracking it
                            for e in 2..=num_tiles {
                                bucket_sets[e].remove(&cell_idx);
                            }

                            // Add to new bucket if still has multiple options
                            if counts[cell_idx] > 1 {
                                bucket_sets[counts[cell_idx]].insert(cell_idx);
                            }
                        }

                        break 'backtrack; // Exit backtracking loop and continue main loop
                    }
                    // If still inconsistent, loop will continue to try another option
                }

                // After successful backtrack, continue main loop
                continue 'outer;
            }

            // No inconsistency - update buckets for all affected cells
            for &cell_idx in &affected_cells {
                // Remove from old bucket if we were tracking it
                for e in 2..=num_tiles {
                    bucket_sets[e].remove(&cell_idx);
                }

                // Add to new bucket if still has multiple options
                if counts[cell_idx] > 1 {
                    bucket_sets[counts[cell_idx]].insert(cell_idx);
                }
            }
        }

        pb.finish_and_clear();

        // Final count verification before building result
        verify_counts(&domains, &mut counts);

        // Build the final map
        let mut result = map.clone();
        for idx in 0..size {
            if !is_ignore[idx] {
                let bits = domains[idx].ones().collect::<Vec<_>>();
                if bits.is_empty() {
                    bail!(
                        "No possibilities for cell at ({}, {})",
                        idx / width,
                        idx % width
                    );
                }
                let tile = bits[0]; // Get the first (and should be only) value
                let r = idx / width;
                let c = idx % width;
                result[(r, c)] = Cell::Fixed(tile);
            }
        }

        println!(
            "Solution found with {} backtrack attempts",
            backtrack_attempts
        );

        Ok(result)
    }
}
