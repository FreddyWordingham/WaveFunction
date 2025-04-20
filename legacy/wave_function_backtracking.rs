use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use crate::{Cell, Map, Rules};

// Structure to represent a decision point for backtracking
#[derive(Clone)]
struct DecisionPoint {
    position: (usize, usize),           // Cell position
    chosen_tile: usize,                 // The tile we selected
    possibilities: Array2<FixedBitSet>, // Full state of the wave function
    cell_versions: Array2<usize>,       // Version tracking for cells
}

#[derive(Clone)]
pub struct WaveFunction<'a> {
    possibilities: Array2<FixedBitSet>,
    rules: &'a Rules,
}

impl<'a> WaveFunction<'a> {
    pub fn new(map: &Map, rules: &'a Rules) -> Self {
        let (height, width) = map.cells().dim();
        let num_tiles = rules.len();

        let full_set = {
            let mut mask = FixedBitSet::with_capacity(num_tiles);
            mask.insert_range(..);
            mask
        };
        let possibilities = Array2::from_shape_fn((height, width), |i| match map.get(i) {
            Cell::Fixed(n) => {
                let mut mask = FixedBitSet::with_capacity(num_tiles);
                mask.insert(n);
                mask
            }
            Cell::Wildcard | Cell::Ignore => full_set.clone(),
        });

        Self {
            possibilities,
            rules,
        }
    }

    /// Gets a clone of the current possibilities
    pub fn get_possibilities(&self) -> Array2<FixedBitSet> {
        self.possibilities.clone()
    }

    /// Sets the possibilities to the given value
    pub fn set_possibilities(&mut self, possibilities: Array2<FixedBitSet>) {
        self.possibilities = possibilities;
    }

    /// Get neighbour coord in `dir`, or None if out‑of‑bounds.
    fn neighbour(&self, (y, x): (usize, usize), dir: Direction) -> Option<(usize, usize)> {
        let (height, width) = self.possibilities.dim();
        match dir {
            Direction::North if y > 0 => Some((y - 1, x)),
            Direction::East if x + 1 < width => Some((y, x + 1)),
            Direction::South if y + 1 < height => Some((y + 1, x)),
            Direction::West if x > 0 => Some((y, x - 1)),
            _ => None,
        }
    }

    /// Remove unsupported values of Xi wrt Xj in `dir`. Returns true if Xi shrank.
    fn revise(&mut self, xi: (usize, usize), xj: (usize, usize), dir: Direction) -> bool {
        let mut changed = false;
        let before = self.possibilities[xi].clone();

        for tile in before.ones() {
            // if no tile in Xj's domain is allowed by `support`, drop `tile`
            if (&self.possibilities[xj] & &self.rules[tile][dir.index::<usize>()]).count_ones(..)
                == 0
            {
                self.possibilities[xi].set(tile, false);
                changed = true;
            }
        }

        changed
    }

    /// Run AC‑3 until arc‑consistency is reached.
    /// Returns false if a contradiction is detected (a cell has no valid options)
    pub fn propagate_ac3(&mut self) -> bool {
        let (height, width) = self.possibilities.dim();
        let mut queue = VecDeque::new();

        // seed queue with every arc (cell → neighbour in each dir)
        for y in 0..height {
            for x in 0..width {
                for &dir in ALL_DIRECTIONS.iter() {
                    if let Some(nbr) = self.neighbour((y, x), dir) {
                        queue.push_back(((y, x), nbr, dir));
                    }
                }
            }
        }

        while let Some((xi, xj, dir)) = queue.pop_front() {
            if self.revise(xi, xj, dir) {
                if self.possibilities[xi].is_empty() {
                    // Contradiction detected
                    return false;
                }
                // enqueue all arcs (xk → xi) except from xj
                for &dir2 in ALL_DIRECTIONS.iter() {
                    if let Some(xk) = self.neighbour(xi, dir2) {
                        if xk != xj {
                            queue.push_back((xk, xi, dir2.opposite()));
                        }
                    }
                }
            }
        }
        true
    }

    /// Propagate from a specific cell and track changes
    /// Returns false if a contradiction is detected
    fn propagate_from(&mut self, start: (usize, usize), changed: &mut Vec<(usize, usize)>) -> bool {
        let mut queue = VecDeque::new();

        // Add neighbors of the starting cell
        for &dir in ALL_DIRECTIONS.iter() {
            if let Some(nbr) = self.neighbour(start, dir) {
                queue.push_back((nbr, start, dir.opposite()));
            }
        }

        while let Some((xi, xj, dir)) = queue.pop_front() {
            if self.revise(xi, xj, dir) {
                // This cell's possibilities changed
                changed.push(xi);

                if self.possibilities[xi].is_empty() {
                    // Contradiction detected
                    return false;
                }

                // Propagate to neighbors
                for &dir2 in ALL_DIRECTIONS.iter() {
                    if let Some(xk) = self.neighbour(xi, dir2) {
                        if xk != xj {
                            queue.push_back((xk, xi, dir2.opposite()));
                        }
                    }
                }
            }
        }
        true
    }

    /// Collapse into a concrete Map with backtracking.
    pub fn collapse<R: Rng>(&mut self, rng: &mut R, weights: &[usize]) -> Map {
        assert!(weights.len() == self.rules.len());

        let (height, width) = self.possibilities.dim();
        let total = (height * width) as u64;

        // Initial propagation to enforce consistency
        if !self.propagate_ac3() {
            panic!("Initial configuration is inconsistent!");
        }

        // Count cells that are already fixed
        let mut fixed = self
            .possibilities
            .iter()
            .filter(|b| b.count_ones(..) == 1)
            .count() as u64;

        // Setup progress tracking
        let pb = ProgressBar::new(total);
        pb.set_position(fixed);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
                .unwrap()
                .progress_chars("##-"),
        );

        // Track cell versions to handle stale entries in heap
        let mut cell_versions = Array2::from_elem((height, width), 0usize);

        // Create entropy-based priority queue (min-heap)
        #[derive(Eq, PartialEq)]
        struct EntropyCell {
            position: (usize, usize),
            entropy: usize,
            version: usize,
        }

        impl Ord for EntropyCell {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.entropy.cmp(&other.entropy)
            }
        }

        impl PartialOrd for EntropyCell {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut entropy_queue = BinaryHeap::new();

        // Initialize queue with all non-fixed cells
        for y in 0..height {
            for x in 0..width {
                let count = self.possibilities[(y, x)].count_ones(..);
                if count > 1 {
                    entropy_queue.push(Reverse(EntropyCell {
                        position: (y, x),
                        entropy: count,
                        version: 0,
                    }));
                }
            }
        }

        // Update progress bar less frequently
        let update_interval = (total / 100).max(1);

        // Keep track of positions whose entropy changed during propagation
        let mut changed_positions = Vec::new();

        // Stack of decision points for backtracking
        let mut decision_stack: Vec<DecisionPoint> = Vec::new();

        // Track failed choices to avoid repeating them
        let mut failed_choices: HashMap<(usize, usize), HashSet<usize>> = HashMap::new();

        // Main loop
        while fixed < total {
            let mut contradicted = false;
            let mut position = None;

            // Get the next cell to collapse
            while let Some(Reverse(cell)) = entropy_queue.pop() {
                let (y, x) = cell.position;

                // Skip if this is a stale entry or already collapsed
                if cell.version != cell_versions[(y, x)]
                    || self.possibilities[(y, x)].count_ones(..) == 1
                {
                    continue;
                }

                // Skip if already fully constrained
                let current_count = self.possibilities[(y, x)].count_ones(..);
                if current_count <= 1 {
                    continue;
                }

                position = Some((y, x));
                break;
            }

            // If no position was found but we're not done, we have a contradiction
            if position.is_none() && fixed < total {
                contradicted = true;
            }

            if let Some((y, x)) = position {
                // Get valid choices for this cell
                let mut choices: Vec<usize> = self.possibilities[(y, x)].ones().collect();

                // Filter out choices that have previously led to contradictions
                if let Some(failed) = failed_choices.get(&(y, x)) {
                    choices.retain(|&choice| !failed.contains(&choice));
                }

                // If no valid choices remain, we have a contradiction
                if choices.is_empty() {
                    contradicted = true;
                } else {
                    // Get weights for valid choices
                    let w: Vec<f64> = choices.iter().map(|&i| weights[i] as f64).collect();

                    // Create weighted distribution
                    if let Ok(dist) = WeightedIndex::new(&w) {
                        let pick_idx = dist.sample(rng);
                        let pick = choices[pick_idx];

                        // Create remaining choices for backtracking
                        let mut remaining = choices.clone();
                        remaining.remove(pick_idx);

                        // Save decision point for backtracking
                        if !remaining.is_empty() {
                            decision_stack.push(DecisionPoint {
                                position: (y, x),
                                chosen_tile: pick,
                                possibilities: self.possibilities.clone(),
                                cell_versions: cell_versions.clone(),
                            });
                        }

                        // Fix the cell to the chosen value
                        let mut m = FixedBitSet::with_capacity(self.rules.len());
                        m.insert(pick);
                        self.possibilities[(y, x)] = m;

                        // Increment version to invalidate any pending entries in queue
                        cell_versions[(y, x)] += 1;

                        // Track changed positions for entropy updates
                        changed_positions.clear();

                        // Propagate constraints from this cell
                        if !self.propagate_from((y, x), &mut changed_positions) {
                            contradicted = true;

                            // Mark this choice as failed for this position
                            failed_choices
                                .entry((y, x))
                                .or_insert_with(HashSet::new)
                                .insert(pick);
                        } else {
                            // Update entropy for changed cells and add to queue
                            for &pos in &changed_positions {
                                let count = self.possibilities[pos].count_ones(..);
                                if count > 1 {
                                    cell_versions[pos] += 1;
                                    entropy_queue.push(Reverse(EntropyCell {
                                        position: pos,
                                        entropy: count,
                                        version: cell_versions[pos],
                                    }));
                                }
                            }

                            fixed += 1;
                            if fixed % update_interval == 0 {
                                pb.set_position(fixed);
                            }
                        }
                    } else {
                        // Unable to create distribution - rare edge case
                        contradicted = true;
                    }
                }
            }

            // Handle contradictions
            if contradicted {
                // Try to backtrack
                if let Some(decision_point) = decision_stack.pop() {
                    // Restore the previous state
                    self.possibilities = decision_point.possibilities;
                    cell_versions = decision_point.cell_versions;
                    fixed = self
                        .possibilities
                        .iter()
                        .filter(|b| b.count_ones(..) == 1)
                        .count() as u64;

                    // Mark this choice as failed
                    failed_choices
                        .entry(decision_point.position)
                        .or_insert_with(HashSet::new)
                        .insert(decision_point.chosen_tile);

                    // Rebuild the entropy queue
                    entropy_queue.clear();
                    for y in 0..height {
                        for x in 0..width {
                            let count = self.possibilities[(y, x)].count_ones(..);
                            if count > 1 {
                                entropy_queue.push(Reverse(EntropyCell {
                                    position: (y, x),
                                    entropy: count,
                                    version: cell_versions[(y, x)],
                                }));
                            }
                        }
                    }

                    pb.set_position(fixed);
                } else {
                    // No more backtracking points - the problem is unsolvable
                    pb.finish_with_message("Failed to find a valid solution!");
                    panic!("Unable to find a valid solution even with backtracking!");
                }
            }
        }

        pb.finish_with_message("Done!");

        // Reconstruct final Map
        let cells = Array2::from_shape_fn((height, width), |idx| {
            let mut iter = self.possibilities[idx].ones();
            match (iter.next(), iter.next()) {
                (Some(i), None) => Cell::Fixed(i),
                _ => Cell::Wildcard,
            }
        });

        Map::new(cells)
    }
}
