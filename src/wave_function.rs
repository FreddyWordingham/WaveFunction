use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use photo::{ALL_DIRECTIONS, Direction};
use rand::{Rng, prelude::IndexedRandom};
use std::collections::VecDeque;

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
    pub fn collapse<R: Rng>(&mut self, rng: &mut R) -> Map {
        let (height, width) = self.possibilities.dim();
        let total_cells = (height * width) as u64;
        // Count how many are already fixed
        let mut fixed = self
            .possibilities
            .iter()
            .filter(|bits| bits.count_ones(..) == 1)
            .count() as u64;

        // create and style the bar
        let pb = ProgressBar::new(total_cells);
        pb.set_position(fixed);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
                .expect("Failed to construct progress bar")
                .progress_chars("##-"),
        );

        // main WFC loop
        loop {
            self.propagate_ac3();

            // pick next cell with >1 options
            let mut best: Option<((usize, usize), usize)> = None;
            for ((y, x), bits) in self.possibilities.indexed_iter() {
                let count = bits.count_ones(..);
                if count > 1 {
                    if best.as_ref().map_or(true, |&(_, c)| count < c) {
                        best = Some(((y, x), count));
                    }
                }
            }

            if let Some(((y, x), _)) = best {
                // collapse it
                let choices: Vec<usize> = self.possibilities[(y, x)].ones().collect();
                let &pick = choices.choose(rng).unwrap();
                let mut mask = FixedBitSet::with_capacity(self.rules.len());
                mask.insert(pick);
                self.possibilities[(y, x)] = mask;
                fixed += 1;
                pb.inc(1);
            } else {
                break;
            }
        }

        pb.finish_with_message("Done!");

        // build final Map
        let cells = Array2::from_shape_fn((height, width), |i| {
            let mut ones = self.possibilities[i].ones();
            match (ones.next(), ones.next()) {
                (Some(n), None) => Cell::Fixed(n),
                _ => Cell::Wildcard,
            }
        });

        Map::new(cells)
    }
}
