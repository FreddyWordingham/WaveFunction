use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::{Cell, Map, Rules};

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
            // if no tile in Xj’s domain is allowed by `support`, drop `tile`
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
    pub fn propagate_ac3(&mut self) {
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
                    panic!("AC‑3 removed all possibilities at {:?}", xi);
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
    }

    /// Collapse into a concrete Map.
    pub fn collapse<R: Rng>(&mut self, rng: &mut R, weights: &[usize]) -> Map {
        assert!(weights.len() == self.rules.len());

        let (height, width) = self.possibilities.dim();
        let total = (height * width) as u64;

        // Initial propagation to enforce consistency
        self.propagate_ac3();

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

        while let Some(Reverse(cell)) = entropy_queue.pop() {
            let (y, x) = cell.position;

            // Skip if this is a stale entry or already collapsed
            if cell.version != cell_versions[(y, x)]
                || self.possibilities[(y, x)].count_ones(..) == 1
            {
                continue;
            }

            // Skip if already fully constrained (might happen after propagation)
            let current_count = self.possibilities[(y, x)].count_ones(..);
            if current_count <= 1 {
                continue;
            }

            // Collapse this cell
            let choices: Vec<usize> = self.possibilities[(y, x)].ones().collect();
            let w: Vec<f64> = choices.iter().map(|&i| weights[i] as f64).collect();
            let dist = WeightedIndex::new(&w).unwrap();
            let pick = choices[dist.sample(rng)];

            // Fix the cell to the chosen value
            let mut m = FixedBitSet::with_capacity(self.rules.len());
            m.insert(pick);
            self.possibilities[(y, x)] = m;

            // Increment version to invalidate any pending entries in queue
            cell_versions[(y, x)] += 1;

            // Track changed positions for entropy updates
            changed_positions.clear();

            // Propagate constraints from this cell
            self.propagate_from((y, x), &mut changed_positions);

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

    // New helper method to propagate from a specific cell and track changes
    fn propagate_from(&mut self, start: (usize, usize), changed: &mut Vec<(usize, usize)>) {
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
                    panic!("Propagation removed all possibilities at {:?}", xi);
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
    }
}
