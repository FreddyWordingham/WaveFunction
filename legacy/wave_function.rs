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

// Improved struct to represent a decision point for backtracking
#[derive(Clone)]
struct DecisionPoint {
    position: (usize, usize),           // Cell position
    chosen_tile: usize,                 // The tile we selected
    possibilities: Array2<FixedBitSet>, // Full state of the wave function
    entropy_cache: Array2<usize>,       // Cached entropy values
    fixed_count: u64,                   // Count of fixed cells
}

// Structure to manage cell entropy and selection
#[derive(Eq, PartialEq)]
struct EntropyCell {
    position: (usize, usize),
    entropy: usize,
    random_offset: u8, // Small random value to break ties randomly
}

impl Ord for EntropyCell {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare primarily by entropy
        self.entropy
            .cmp(&other.entropy)
            // If entropy is equal, use random tie-breaker
            .then_with(|| self.random_offset.cmp(&other.random_offset))
    }
}

impl PartialOrd for EntropyCell {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone)]
pub struct WaveFunction<'a> {
    possibilities: Array2<FixedBitSet>,
    entropy_cache: Array2<usize>, // Cache entropy values
    rules: &'a Rules,
    propagation_buffer: VecDeque<((usize, usize), (usize, usize), Direction)>, // Reusable buffer
    changed_positions: Vec<(usize, usize)>, // Reusable buffer for changed positions
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

        // Initialize entropy cache
        let entropy_cache =
            Array2::from_shape_fn((height, width), |idx| possibilities[idx].count_ones(..));

        // Pre-allocate buffers
        let propagation_buffer = VecDeque::with_capacity(height * width * 4);
        let changed_positions = Vec::with_capacity(height * width);

        Self {
            possibilities,
            entropy_cache,
            rules,
            propagation_buffer,
            changed_positions,
        }
    }

    /// Get neighbour coord in `dir`, or None if out‑of‑bounds.
    #[inline]
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
    #[inline]
    fn revise(&mut self, xi: (usize, usize), xj: (usize, usize), dir: Direction) -> bool {
        let mut changed = false;
        let before = self.possibilities[xi].clone();
        let dir_idx = dir.index::<usize>();

        for tile in before.ones() {
            // Fast check if any tile in Xj's domain is allowed by rules
            if (&self.possibilities[xj] & &self.rules[tile][dir_idx]).count_ones(..) == 0 {
                self.possibilities[xi].set(tile, false);
                changed = true;
            }
        }

        if changed {
            // Update entropy cache
            self.entropy_cache[xi] = self.possibilities[xi].count_ones(..);
        }

        changed
    }

    /// Run AC‑3 until arc‑consistency is reached.
    /// Returns false if a contradiction is detected (a cell has no valid options)
    pub fn propagate_ac3(&mut self) -> bool {
        let (height, width) = self.possibilities.dim();
        self.propagation_buffer.clear();

        // seed queue with every arc (cell → neighbour in each dir)
        for y in 0..height {
            for x in 0..width {
                for &dir in ALL_DIRECTIONS.iter() {
                    if let Some(nbr) = self.neighbour((y, x), dir) {
                        self.propagation_buffer.push_back(((y, x), nbr, dir));
                    }
                }
            }
        }

        while let Some((xi, xj, dir)) = self.propagation_buffer.pop_front() {
            if self.revise(xi, xj, dir) {
                if self.possibilities[xi].is_empty() {
                    // Contradiction detected
                    return false;
                }
                // enqueue all arcs (xk → xi) except from xj
                for &dir2 in ALL_DIRECTIONS.iter() {
                    if let Some(xk) = self.neighbour(xi, dir2) {
                        if xk != xj {
                            self.propagation_buffer.push_back((xk, xi, dir2.opposite()));
                        }
                    }
                }
            }
        }
        true
    }

    /// Propagate from a specific cell
    /// Returns false if a contradiction is detected
    #[inline]
    fn propagate_from(&mut self, start: (usize, usize)) -> bool {
        self.propagation_buffer.clear();
        self.changed_positions.clear();

        // Add neighbors of the starting cell
        for &dir in ALL_DIRECTIONS.iter() {
            if let Some(nbr) = self.neighbour(start, dir) {
                self.propagation_buffer
                    .push_back((nbr, start, dir.opposite()));
            }
        }

        while let Some((xi, xj, dir)) = self.propagation_buffer.pop_front() {
            if self.revise(xi, xj, dir) {
                // This cell's possibilities changed
                self.changed_positions.push(xi);

                if self.possibilities[xi].is_empty() {
                    // Contradiction detected
                    return false;
                }

                // Propagate to neighbors
                for &dir2 in ALL_DIRECTIONS.iter() {
                    if let Some(xk) = self.neighbour(xi, dir2) {
                        if xk != xj {
                            self.propagation_buffer.push_back((xk, xi, dir2.opposite()));
                        }
                    }
                }
            }
        }

        true
    }

    /// Finds the cell with minimum entropy
    fn find_min_entropy_cell<R: Rng>(&self, rng: &mut R) -> Option<(usize, usize)> {
        let (height, width) = self.possibilities.dim();

        let mut min_entropy = usize::MAX;
        let mut candidates = Vec::new();

        // First pass: find minimum entropy value
        for y in 0..height {
            for x in 0..width {
                let entropy = self.entropy_cache[(y, x)];
                if entropy > 1 {
                    // Only consider uncollapsed cells
                    if entropy < min_entropy {
                        min_entropy = entropy;
                        candidates.clear();
                        candidates.push((y, x));
                    } else if entropy == min_entropy {
                        candidates.push((y, x));
                    }
                }
            }
        }

        // If we found any candidates, pick one randomly
        if !candidates.is_empty() {
            return Some(candidates[rng.random_range(0..candidates.len())]);
        }

        None
    }

    /// Rebuild the entropy priority queue
    fn rebuild_priority_queue<R: Rng>(
        &self,
        rng: &mut R,
        queue: &mut BinaryHeap<Reverse<EntropyCell>>,
    ) {
        let (height, width) = self.possibilities.dim();
        queue.clear();

        for y in 0..height {
            for x in 0..width {
                let entropy = self.entropy_cache[(y, x)];
                if entropy > 1 {
                    queue.push(Reverse(EntropyCell {
                        position: (y, x),
                        entropy,
                        random_offset: rng.random(),
                    }));
                }
            }
        }
    }

    /// Count fixed cells
    fn count_fixed_cells(&self) -> u64 {
        let (height, width) = self.possibilities.dim();
        let mut count = 0;

        for y in 0..height {
            for x in 0..width {
                if self.entropy_cache[(y, x)] == 1 {
                    count += 1;
                }
            }
        }

        count
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

        // Update entropy cache after initial propagation
        for y in 0..height {
            for x in 0..width {
                self.entropy_cache[(y, x)] = self.possibilities[(y, x)].count_ones(..);
            }
        }

        // Count cells that are already fixed
        let mut fixed = self.count_fixed_cells();

        // Setup progress tracking
        let pb = ProgressBar::new(total);
        pb.set_position(fixed);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
                .unwrap()
                .progress_chars("##-"),
        );

        // Create entropy-based priority queue (min-heap)
        let mut entropy_queue = BinaryHeap::new();
        self.rebuild_priority_queue(rng, &mut entropy_queue);

        // Update progress bar less frequently
        let update_interval = (total / 100).max(1);

        // Stack of decision points for backtracking
        let mut decision_stack: Vec<DecisionPoint> = Vec::new();

        // Track failed choices to avoid repeating them
        let mut failed_choices: HashMap<(usize, usize), HashSet<usize>> = HashMap::new();

        // Main loop
        while fixed < total {
            let mut contradicted = false;
            let position = self.find_min_entropy_cell(rng);

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
                    // Use weighted selection
                    let w: Vec<f64> = choices.iter().map(|&i| weights[i] as f64).collect();

                    // Create weighted distribution
                    if let Ok(dist) = WeightedIndex::new(&w) {
                        let pick_idx = dist.sample(rng);
                        let pick = choices[pick_idx];

                        // Save decision point for backtracking if there are multiple choices
                        if choices.len() > 1 {
                            decision_stack.push(DecisionPoint {
                                position: (y, x),
                                chosen_tile: pick,
                                possibilities: self.possibilities.clone(),
                                entropy_cache: self.entropy_cache.clone(),
                                fixed_count: fixed,
                            });
                        }

                        // Fix the cell to the chosen value
                        let mut m = FixedBitSet::with_capacity(self.rules.len());
                        m.insert(pick);
                        self.possibilities[(y, x)] = m;
                        self.entropy_cache[(y, x)] = 1; // Update entropy cache

                        // Propagate constraints from this cell
                        if !self.propagate_from((y, x)) {
                            contradicted = true;

                            // Mark this choice as failed for this position
                            failed_choices
                                .entry((y, x))
                                .or_insert_with(HashSet::new)
                                .insert(pick);
                        } else {
                            // Count newly fixed cells
                            let mut new_fixed = 1; // This cell
                            for &pos in &self.changed_positions {
                                if self.entropy_cache[pos] == 1 {
                                    new_fixed += 1;
                                }
                            }

                            fixed += new_fixed;
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
                    self.entropy_cache = decision_point.entropy_cache;
                    fixed = decision_point.fixed_count;

                    // Mark this choice as failed
                    failed_choices
                        .entry(decision_point.position)
                        .or_insert_with(HashSet::new)
                        .insert(decision_point.chosen_tile);

                    // No need to rebuild the whole queue - will use find_min_entropy_cell

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
            if self.entropy_cache[idx] == 1 {
                let index = self.possibilities[idx].ones().next().unwrap();
                Cell::Fixed(index)
            } else {
                Cell::Wildcard
            }
        });

        Map::new(cells)
    }
}
