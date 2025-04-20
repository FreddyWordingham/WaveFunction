use anyhow::{Result, anyhow};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use rand::Rng;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use rand::seq::SliceRandom;
use std::collections::VecDeque;

use crate::{Map, Tile, Tileset};

#[derive(Clone)]
struct Neighbor {
    idx: usize,
    opp: usize,
}

#[derive(Clone)]
pub struct WaveFunction {
    width: usize,
    height: usize,
    ntiles: usize,
    cells: Vec<FixedBitSet>,       // flattened possibilities
    mask: Vec<bool>,               // flattened mask
    counts: Vec<usize>,            // cached popcounts
    neighbors: Vec<Vec<Neighbor>>, // precomputed neighbor lists
}

impl WaveFunction {
    pub fn new(map: &Map, tileset: &Tileset) -> Self {
        let (height, width) = map.tiles().dim();
        let ntiles = tileset.len();
        let size = height * width;

        // init cells, counts, mask
        let mut cells = Vec::with_capacity(size);
        let mut counts = Vec::with_capacity(size);
        let mut mask = Vec::with_capacity(size);
        for tile in map.tiles() {
            let mut bs = FixedBitSet::with_capacity(ntiles);
            match tile {
                Tile::Fixed(i) => {
                    bs.insert(*i);
                    counts.push(1);
                    mask.push(true);
                }
                Tile::Ignore => {
                    bs.insert_range(0..ntiles);
                    counts.push(ntiles);
                    mask.push(false);
                }
                _ => {
                    bs.insert_range(0..ntiles);
                    counts.push(ntiles);
                    mask.push(true);
                }
            }
            cells.push(bs);
        }
        // precompute neighbors
        let mut neighbors: Vec<Vec<Neighbor>> = (0..size).map(|_| Vec::new()).collect();
        for r in 0..height {
            for c in 0..width {
                let idx = r * width + c;
                for &(dy, dx, dir) in &[(-1, 0, 0), (0, 1, 1), (1, 0, 2), (0, -1, 3)] {
                    let nr = (r as isize + dy) as usize;
                    let nc = (c as isize + dx) as usize;
                    if nr < height && nc < width {
                        let nidx = nr * width + nc;
                        let opp = (dir + 2) & 3;
                        neighbors[idx].push(Neighbor { idx: nidx, opp });
                    }
                }
            }
        }

        WaveFunction {
            width,
            height,
            ntiles,
            cells,
            mask,
            counts,
            neighbors,
        }
    }

    /// Revise the arc y->x in direction dir, updating counts
    fn revise(&mut self, y: usize, x: usize, dir: usize, tileset: &Tileset) -> Result<bool> {
        if !self.mask[y] || !self.mask[x] {
            return Ok(false);
        }
        let before = self.counts[y];
        // borrow cell and neighbor via split_at_mut
        let slice = self.cells.as_mut_slice();
        if y <= x {
            let (left, right) = slice.split_at_mut(x);
            let cell = &mut left[y];
            let neigh = &right[0];
            for t in cell.ones().collect::<Vec<_>>() {
                let allowed = &tileset.rules().masks()[t][dir];
                if allowed.intersection(neigh).next().is_none() {
                    cell.set(t, false);
                    self.counts[y] -= 1;
                }
            }
        } else {
            let (left, right) = slice.split_at_mut(y);
            let cell = &mut right[0];
            let neigh = &left[x];
            for t in cell.ones().collect::<Vec<_>>() {
                let allowed = &tileset.rules().masks()[t][dir];
                if allowed.intersection(neigh).next().is_none() {
                    cell.set(t, false);
                    self.counts[y] -= 1;
                }
            }
        }
        Ok(self.counts[y] < before)
    }

    /// Perform full collapse with bucketed PQ and merged AC-3 propagation
    pub fn collapse(&mut self, rng: &mut impl Rng, tileset: &Tileset) -> Result<Map> {
        // bucketed queues by entropy
        let max_e = self.ntiles;
        let mut buckets = vec![VecDeque::new(); max_e + 1];
        let mut min_e = usize::MAX;
        for i in 0..self.cells.len() {
            let e = self.counts[i];
            if self.mask[i] && e > 1 {
                buckets[e].push_back(i);
                min_e = min_e.min(e);
            }
        }

        // seed initial AC-3 queue with fixed cells
        let mut queue = VecDeque::new();
        for i in 0..self.cells.len() {
            if self.counts[i] == 1 && self.mask[i] {
                for n in &self.neighbors[i] {
                    if self.mask[n.idx] {
                        queue.push_back((n.idx, i, n.opp));
                    }
                }
            }
        }
        // initial propagation
        while let Some((y, x, dir)) = queue.pop_front() {
            if self.revise(y, x, dir, tileset)? {
                let e = self.counts[y];
                if e > 1 {
                    buckets[e].push_back(y);
                    min_e = min_e.min(e);
                }
                for n in &self.neighbors[y] {
                    if self.mask[n.idx] {
                        queue.push_back((n.idx, y, n.opp));
                    }
                }
            }
        }

        // progress bar
        let total = (self.width * self.height) as u64;
        let pb = ProgressBar::new(total).with_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )?
                .progress_chars("#>-"),
        );
        // collapse loop
        while let Some(ent) = (2..=max_e).find(|&e| !buckets[e].is_empty()) {
            let idx = buckets[ent].pop_front().unwrap();

            // pick random tile
            // let opts = self.cells[idx].ones().collect::<Vec<_>>();
            // let weights = opts
            //     .iter()
            //     .map(|&t| tileset.frequency(t))
            //     .collect::<Vec<_>>();
            // let dist = WeightedIndex::new(&weights).map_err(|e| anyhow!(e))?;
            // let pick = opts[dist.sample(rng)];
            let opts = self.cells[idx].ones().collect::<Vec<_>>();
            if opts.is_empty() {
                return Err(anyhow!("No valid tiles at cell {}—WFC contradiction", idx));
            }
            // build weights
            let weights: Vec<usize> = opts.iter().map(|&t| tileset.frequency(t)).collect();
            let pick = if weights.iter().sum::<usize>() == 0 {
                // all-zero frequencies: uniform fallback
                opts[rng.random_range(0..opts.len())]
            } else {
                // safe to build the distribution
                let dist = WeightedIndex::new(&weights)
                    .map_err(|e| anyhow!("Invalid weights at cell {}: {}", idx, e))?;
                opts[dist.sample(rng)]
            };

            // collapse
            self.cells[idx].clear();
            self.cells[idx].insert(pick);
            self.counts[idx] = 1;
            pb.inc(1);
            // propagate just this collapse
            let mut queue = VecDeque::new();
            for n in &self.neighbors[idx] {
                if self.mask[n.idx] {
                    queue.push_back((n.idx, idx, n.opp));
                }
            }
            while let Some((y, x, dir)) = queue.pop_front() {
                if self.revise(y, x, dir, tileset)? {
                    let ce = self.counts[y];
                    if ce > 1 {
                        buckets[ce].push_back(y);
                    }
                    for n in &self.neighbors[y] {
                        if self.mask[n.idx] {
                            queue.push_back((n.idx, y, n.opp));
                        }
                    }
                }
            }
        }

        pb.finish();

        // build map
        let tiles = self
            .cells
            .iter()
            .map(|bs| bs.ones().next().unwrap())
            .map(Tile::Fixed)
            .collect::<Vec<_>>();
        let arr = Array2::from_shape_vec((self.height, self.width), tiles)?;
        Ok(Map::new(arr))
    }

    /// Full recursive backtracking solver
    pub fn solve<R: Rng + ?Sized>(&self, rng: &mut R, tileset: &Tileset) -> Option<Map> {
        // 1) Clone state
        let mut wf = self.clone();

        // 2) Initial AC‑3 propagation
        if wf.initial_propagate(tileset).is_err() {
            return None;
        }

        // 3) If complete, build map
        if wf.counts.iter().all(|&c| c <= 1) {
            return wf.build_map().ok();
        }

        // 4) Pick lowest‑entropy cell
        let idx = wf
            .counts
            .iter()
            .enumerate()
            .filter(|&(i, &c)| wf.mask[i] && c > 1)
            .min_by_key(|&(_, &c)| c)
            .map(|(i, _)| i)?;

        // 5) Try each possibility in random order
        let mut options = wf.cells[idx].ones().collect::<Vec<_>>();
        options.shuffle(rng);
        for &t in &options {
            let mut branch = wf.clone();
            branch.cells[idx].clear();
            branch.cells[idx].insert(t);
            branch.counts[idx] = 1;
            if let Some(sol) = branch.solve(rng, tileset) {
                return Some(sol);
            }
        }
        None
    }

    /// Collapse with backtracking, yielding an error if unsolvable
    pub fn collapse_with_backtracking<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
        tileset: &Tileset,
    ) -> Result<Map> {
        // seed self with initial propagate
        self.initial_propagate(tileset)?;
        if let Some(map) = self.solve(rng, tileset) {
            Ok(map)
        } else {
            Err(anyhow!("No solution found via backtracking"))
        }
    }

    // Helper: initial AC‑3 propagation
    fn initial_propagate(&mut self, tileset: &Tileset) -> Result<()> {
        let mut queue = VecDeque::new();
        // seed fixed bits
        for i in 0..self.cells.len() {
            if self.counts[i] == 1 && self.mask[i] {
                for n in &self.neighbors[i] {
                    if self.mask[n.idx] {
                        queue.push_back((n.idx, i, n.opp));
                    }
                }
            }
        }
        while let Some((y, x, dir)) = queue.pop_front() {
            if self.revise(y, x, dir, tileset)? {
                if self.counts[y] == 0 {
                    return Err(anyhow!("AC‑3 contradiction during init"));
                }
                for n in &self.neighbors[y] {
                    if self.mask[n.idx] {
                        queue.push_back((n.idx, y, n.opp));
                    }
                }
            }
        }
        Ok(())
    }

    // Helper: build Map from current state
    fn build_map(&self) -> Result<Map> {
        let tiles = self
            .cells
            .iter()
            .map(|bs| bs.ones().next().unwrap())
            .map(Tile::Fixed)
            .collect::<Vec<_>>();
        let arr = Array2::from_shape_vec((self.height, self.width), tiles)
            .map_err(|e: ndarray::ShapeError| anyhow!(e))?;
        Ok(Map::new(arr))
    }
}
