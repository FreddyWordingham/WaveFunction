use anyhow::{Result, bail};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use photo::Direction;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use std::collections::VecDeque;

use crate::{Cell, Map, Rules, WaveFunction};

const DELTAS: [(isize, isize); 4] = [(-1, 0), (0, 1), (1, 0), (0, -1)];

pub struct WaveFunctionBasic;

impl WaveFunction for WaveFunctionBasic {
    /// Collapses a map using the Wave Function Collapse algorithm
    /// Returns a new map with all wildcards collapsed to fixed values.
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map> {
        let (height, width) = {
            let shape = map.cells().shape();
            (shape[0], shape[1])
        };
        let num_tiles = rules.len();
        let size = height * width;

        // Flattened domains; ignore cells get an empty bitset but are skipped below
        let mut domains: Vec<FixedBitSet> = Vec::with_capacity(size);
        let mut is_ignore = vec![false; size];
        for idx in 0..size {
            let r = idx / width;
            let c = idx % width;
            match map[(r, c)] {
                Cell::Ignore => {
                    let bs = FixedBitSet::with_capacity(num_tiles);
                    domains.push(bs);
                    is_ignore[idx] = true;
                }
                Cell::Wildcard => {
                    let mut bs = FixedBitSet::with_capacity(num_tiles);
                    bs.insert_range(..num_tiles);
                    domains.push(bs);
                }
                Cell::Fixed(i) => {
                    let mut bs = FixedBitSet::with_capacity(num_tiles);
                    bs.insert(i);
                    domains.push(bs);
                }
            }
        }

        // Helper: run AC³ on the current domains, starting from `queue`
        let mut queue = VecDeque::new();
        let mut enqueue_all = || {
            for xi in 0..size {
                if is_ignore[xi] {
                    continue;
                }
                let (r, c) = (xi / width, xi % width);
                for (d_idx, &(dr, dc)) in DELTAS.iter().enumerate() {
                    let nr = r.wrapping_add(dr as usize);
                    let nc = c.wrapping_add(dc as usize);
                    if nr < height && nc < width {
                        let xj = nr * width + nc;
                        if !is_ignore[xj] {
                            queue.push_back((xi, xj, d_idx));
                        }
                    }
                }
            }
        };

        fn revise(
            domains: &mut [FixedBitSet],
            rules: &Rules,
            xi: usize,
            xj: usize,
            d_idx: usize,
        ) -> bool {
            let mut removed = Vec::new();
            for u in domains[xi].ones() {
                let mut ok = false;
                for v in domains[xj].ones() {
                    if rules.masks()[u][d_idx].contains(v) {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    removed.push(u);
                }
            }
            if removed.is_empty() {
                false
            } else {
                for u in removed {
                    domains[xi].remove(u);
                }
                true
            }
        }

        // Full AC3 propagation
        enqueue_all();
        while let Some((xi, xj, d_idx)) = queue.pop_front() {
            if revise(&mut domains, rules, xi, xj, d_idx) {
                if domains[xi].is_empty() {
                    bail!(
                        "No valid tiles remain at cell ({}, {})",
                        xi / width,
                        xi % width
                    );
                }
                // propagate change to neighbors of xi (except xj)
                let (r, c) = (xi / width, xi % width);
                for (d2, &(dr, dc)) in DELTAS.iter().enumerate() {
                    let nr = r.wrapping_add(dr as usize);
                    let nc = c.wrapping_add(dc as usize);
                    if nr < height && nc < width {
                        let xk = nr * width + nc;
                        if xk != xj && !is_ignore[xk] {
                            let opp_dir = Direction::from_index((d2 + 2) % 4);
                            queue.push_back((xk, xi, opp_dir.index::<usize>()));
                        }
                    }
                }
            }
        }

        // how many to collapse?
        let total = domains
            .iter()
            .enumerate()
            .filter(|(i, dom)| !is_ignore[*i] && dom.count_ones(..) > 1)
            .count();

        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                .unwrap()
                .progress_chars("##-"),
        );

        // Main loop: pick a cell with >1 possibility, collapse it, re-propagate
        while let Some((best_idx, _)) = domains
            .iter()
            .enumerate()
            .filter(|(i, dom)| !is_ignore[*i] && dom.count_ones(..) > 1)
            .min_by_key(|(_, dom)| dom.count_ones(..))
        {
            // pick one tile weighted by frequency
            let options: Vec<usize> = domains[best_idx].ones().collect();
            let weights: Vec<usize> = options.iter().map(|&t| rules.frequencies()[t]).collect();
            let dist = WeightedIndex::new(&weights).unwrap();
            let choice = options[dist.sample(rng)];

            pb.inc(1);

            // fix it
            domains[best_idx].clear();
            domains[best_idx].insert(choice);

            // propagate from this collapse
            let (r, c) = (best_idx / width, best_idx % width);
            for (d_idx, &(dr, dc)) in DELTAS.iter().enumerate() {
                let nr = r.wrapping_add(dr as usize);
                let nc = c.wrapping_add(dc as usize);
                if nr < height && nc < width {
                    let neighbor = nr * width + nc;
                    if !is_ignore[neighbor] {
                        let opp = Direction::from_index((d_idx + 2) % 4).index::<usize>();
                        queue.push_back((neighbor, best_idx, opp));
                    }
                }
            }
            while let Some((xi, xj, d_idx)) = queue.pop_front() {
                if revise(&mut domains, rules, xi, xj, d_idx) {
                    if domains[xi].is_empty() {
                        bail!(
                            "No valid tiles remain after collapse at ({}, {})",
                            xi / width,
                            xi % width
                        );
                    }
                    let (r2, c2) = (xi / width, xi % width);
                    for (d2, &(dr, dc)) in DELTAS.iter().enumerate() {
                        let nr = r2.wrapping_add(dr as usize);
                        let nc = c2.wrapping_add(dc as usize);
                        if nr < height && nc < width {
                            let xk = nr * width + nc;
                            if xk != xj && !is_ignore[xk] {
                                let opp_dir = Direction::from_index((d2 + 2) % 4);
                                queue.push_back((xk, xi, opp_dir.index::<usize>()));
                            }
                        }
                    }
                }
            }
        }
        pb.finish_and_clear();

        // Build the final map
        let mut result = map.clone();
        for idx in 0..size {
            if !is_ignore[idx] {
                let mut bits = domains[idx].ones();
                let tile = bits.next().unwrap(); // <-- pull the single value
                let r = idx / width;
                let c = idx % width;
                result[(r, c)] = Cell::Fixed(tile);
            }
        }
        Ok(result)
    }

    // /// Collapses a map using backtracking + AC³.
    // pub fn collapse_with_backtracking<R: Rng>(
    //     map: &Map,
    //     rules: &Rules,
    //     rng: &mut R,
    // ) -> Result<Map> {
    //     let (h, w) = {
    //         let shape = map.cells().shape();
    //         (shape[0], shape[1])
    //     };
    //     let n_tiles = rules.len();
    //     let size = h * w;

    //     // initial domains & ignore
    //     let mut domains = Vec::with_capacity(size);
    //     let mut is_ignore = vec![false; size];
    //     for i in 0..size {
    //         match map.get((i / w, i % w)) {
    //             Cell::Ignore => {
    //                 domains.push(FixedBitSet::with_capacity(n_tiles));
    //                 is_ignore[i] = true;
    //             }
    //             Cell::Fixed(t) => {
    //                 let mut bs = FixedBitSet::with_capacity(n_tiles);
    //                 bs.insert(t);
    //                 domains.push(bs);
    //             }
    //             Cell::Wildcard => {
    //                 let mut bs = FixedBitSet::with_capacity(n_tiles);
    //                 bs.insert_range(..n_tiles);
    //                 domains.push(bs);
    //             }
    //         }
    //     }

    //     // progress bar over number of decisions
    //     let total = domains
    //         .iter()
    //         .enumerate()
    //         .filter(|(i, d)| !is_ignore[*i] && d.count_ones(..) > 1)
    //         .count() as u64;
    //     let pb = ProgressBar::new(total);
    //     pb.set_style(
    //         ProgressStyle::with_template("{bar:40.green/white} {pos}/{len} cells")
    //             .unwrap()
    //             .progress_chars("##-"),
    //     );

    //     // record of all removals, so we can undo
    //     struct Change {
    //         idx: usize,
    //         removed: Vec<usize>,
    //     }

    //     // AC³ that pushes every domain‐removal into `trail`
    //     fn ac3_with_trail(
    //         dom: &mut [FixedBitSet],
    //         ign: &[bool],
    //         h: usize,
    //         w: usize,
    //         rules: &Rules,
    //         trail: &mut Vec<Change>,
    //     ) -> bool {
    //         let mut queue = VecDeque::new();
    //         for xi in 0..dom.len() {
    //             if ign[xi] {
    //                 continue;
    //             }
    //             let r = xi / w;
    //             let c = xi % w;
    //             for (d_idx, &(dr, dc)) in DELTAS.iter().enumerate() {
    //                 let nr = r.wrapping_add(dr as usize);
    //                 let nc = c.wrapping_add(dc as usize);
    //                 if nr < h && nc < w {
    //                     let xj = nr * w + nc;
    //                     if !ign[xj] {
    //                         queue.push_back((xi, xj, d_idx));
    //                     }
    //                 }
    //             }
    //         }
    //         while let Some((xi, xj, d)) = queue.pop_front() {
    //             let mut removed = Vec::new();
    //             for u in dom[xi].ones() {
    //                 let mut ok = false;
    //                 for v in dom[xj].ones() {
    //                     if rules.masks()[u][d].contains(v) {
    //                         ok = true;
    //                         break;
    //                     }
    //                 }
    //                 if !ok {
    //                     removed.push(u);
    //                 }
    //             }
    //             if removed.is_empty() {
    //                 continue;
    //             }
    //             for &u in &removed {
    //                 dom[xi].remove(u);
    //             }
    //             trail.push(Change { idx: xi, removed });
    //             if dom[xi].is_empty() {
    //                 return false;
    //             }
    //             // enqueue neighbors of xi
    //             let r = xi / w;
    //             let c = xi % w;
    //             for (d2, &(dr, dc)) in DELTAS.iter().enumerate() {
    //                 let nr = r.wrapping_add(dr as usize);
    //                 let nc = c.wrapping_add(dc as usize);
    //                 if nr < h && nc < w {
    //                     let xk = nr * w + nc;
    //                     if xk != xj && !ign[xk] {
    //                         let opp = Direction::from_index((d2 + 2) % 4).index::<usize>();
    //                         queue.push_back((xk, xi, opp));
    //                     }
    //                 }
    //             }
    //         }
    //         true
    //     }

    //     // depth‐first search, returns true on success
    //     fn dfs<R: Rng>(
    //         dom: &mut [FixedBitSet],
    //         ign: &[bool],
    //         h: usize,
    //         w: usize,
    //         rules: &Rules,
    //         rng: &mut R,
    //         pb: &ProgressBar,
    //         trail: &mut Vec<Change>,
    //     ) -> bool {
    //         // pick the cell with minimum remaining values
    //         let idx_opt = dom
    //             .iter()
    //             .enumerate()
    //             .filter(|(i, d)| !ign[*i] && d.count_ones(..) > 1)
    //             .min_by_key(|(_, d)| d.count_ones(..))
    //             .map(|(i, _)| i);
    //         if idx_opt.is_none() {
    //             return true; // all singletons
    //         }
    //         let i = idx_opt.unwrap();

    //         let mut opts: Vec<usize> = dom[i].ones().collect();
    //         opts.shuffle(rng);
    //         for &tile in &opts {
    //             // save original domain for cell i
    //             let backup = dom[i].clone();
    //             // marker so we know where to stop undoing
    //             trail.push(Change {
    //                 idx: i,
    //                 removed: Vec::new(),
    //             });

    //             // assign and record one decision
    //             dom[i].clear();
    //             dom[i].insert(tile);
    //             pb.inc(1);

    //             // propagate and recurse
    //             if ac3_with_trail(dom, ign, h, w, rules, trail)
    //                 && dfs(dom, ign, h, w, rules, rng, pb, trail)
    //             {
    //                 return true;
    //             }

    //             // undo everything up to the marker
    //             while let Some(Change { idx, removed }) = trail.pop() {
    //                 if removed.is_empty() && idx == i {
    //                     dom[idx] = backup;
    //                     break;
    //                 }
    //                 for u in removed {
    //                     dom[idx].insert(u);
    //                 }
    //             }
    //             // rewind progress bar
    //             let pos = pb.position().saturating_sub(1);
    //             pb.set_position(pos);
    //         }
    //         false
    //     }

    //     // initial propagation
    //     let mut trail = Vec::new();
    //     if !ac3_with_trail(&mut domains, &is_ignore, h, w, rules, &mut trail) {
    //         bail!("No solution from initial AC³");
    //     }

    //     // search
    //     if !dfs(&mut domains, &is_ignore, h, w, rules, rng, &pb, &mut trail) {
    //         bail!("No solution found");
    //     }

    //     pb.finish_and_clear();

    //     // build result
    //     let mut result = map.clone();
    //     for (i, dom) in domains.into_iter().enumerate() {
    //         if is_ignore[i] {
    //             continue;
    //         }
    //         let t = dom.ones().next().unwrap();
    //         result.set((i / w, i % w), Cell::Fixed(t));
    //     }
    //     Ok(result)
    // }
}
