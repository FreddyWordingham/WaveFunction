use anyhow::{Result, anyhow};
use fixedbitset::FixedBitSet;
use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};
use std::collections::VecDeque;

use crate::{Map, Tile, Tileset};

const NEIGHBOURS: &[(isize, isize, usize)] = &[
    (-1, 0, 0), // North
    (0, 1, 1),  // East
    (1, 0, 2),  // South
    (0, -1, 3), // West
];

pub struct WaveFunction<'a> {
    possibilities: Array2<FixedBitSet>,
    mask: Array2<bool>,
    tileset: &'a Tileset,
}

impl<'a> WaveFunction<'a> {
    pub fn new(map: &Map, tileset: &'a Tileset) -> Self {
        let ntiles = tileset.len();
        let shape = map.tiles().dim();
        // start all-ones
        let mut poss = Array2::from_shape_fn(shape, |_| {
            let mut bs = FixedBitSet::with_capacity(ntiles);
            bs.insert_range(0..ntiles);
            bs
        });
        let mut mask = map.tiles().mapv(|t| !matches!(t, Tile::Ignore));

        // honour fixed tiles
        for ((x, y), tile) in map.tiles().indexed_iter() {
            if let Tile::Fixed(i) = tile {
                let mut bs = FixedBitSet::with_capacity(ntiles);
                bs.insert(*i);
                poss[(x, y)] = bs;
                mask[(x, y)] = true;
            }
        }

        WaveFunction {
            possibilities: poss,
            mask,
            tileset,
        }
    }

    pub fn ac3(&mut self) -> Result<()> {
        let (height, width) = self.possibilities.dim();
        let mut queue = VecDeque::new();
        for y in 0..height {
            for x in 0..width {
                if !self.mask[(y, x)] {
                    continue;
                }
                for &(dy, dx, dir) in NEIGHBOURS {
                    let ny = (y as isize + dy) as usize;
                    let nx = (x as isize + dx) as usize;
                    if ny < height && nx < width && self.mask[(ny, nx)] {
                        queue.push_back(((y, x), (ny, nx, dir)));
                    }
                }
            }
        }

        while let Some(((y, x), (ny, nx, dir))) = queue.pop_front() {
            if self.revise((y, x), (ny, nx), dir)? {
                if self.possibilities[(y, x)].is_empty() {
                    return Err(anyhow!("AC‑3 failed: ({},{}) no possibilities", x, y));
                }
                for &(ddy, ddx, dir2) in NEIGHBOURS {
                    let py = (y as isize + ddy) as usize;
                    let px = (x as isize + ddx) as usize;
                    if py < height && px < width && (py, px) != (ny, nx) && self.mask[(py, px)] {
                        let opposite = (dir2 + 2) & 3;
                        queue.push_back(((py, px), (y, x, opposite)));
                    }
                }
            }
        }
        Ok(())
    }

    fn revise(
        &mut self,
        (y, x): (usize, usize),
        (ny, nx): (usize, usize),
        dir: usize,
    ) -> Result<bool> {
        if !self.mask[(y, x)] || !self.mask[(ny, nx)] {
            return Ok(false);
        }

        // 1) Grab the contiguous slice and compute linear indices:
        let (_, width) = self.possibilities.dim();
        let slice = self.possibilities.as_slice_mut().unwrap();
        let idx = y * width + x;
        let nidx = ny * width + nx;

        // 2) Split at the later index so we can get two borrows:
        let (cell, neigh): (&mut FixedBitSet, &FixedBitSet) = if idx <= nidx {
            let (left, right) = slice.split_at_mut(nidx);
            (&mut left[idx], &right[0])
        } else {
            let (left, right) = slice.split_at_mut(idx);
            (&mut right[0], &left[nidx])
        };

        // 3) Intersection check:
        let rules = self.tileset.rules().masks();
        let mut changed = false;
        for tile in cell.ones().collect::<Vec<_>>() {
            let allowed = &rules[tile][dir];
            if allowed.intersection(neigh).next().is_none() {
                cell.set(tile, false);
                changed = true;
            }
        }
        Ok(changed)
    }

    fn all_collapsed(&self) -> bool {
        self.possibilities
            .indexed_iter()
            .all(|((x, y), bs)| !self.mask[(x, y)] || bs.count_ones(..) == 1)
    }

    pub fn collapse<R: Rng>(&mut self, rng: &mut R) -> Result<Map> {
        // 1) Initial propagation
        self.ac3()?;

        let (height, width) = self.possibilities.dim();
        let total = (height * width) as u64;
        let pb = ProgressBar::new(total).with_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )?
                .progress_chars("#>-"),
        );

        // 2) As long as something is unfixed…
        while !self.all_collapsed() {
            // find the cell with minimum entropy >1
            let ((row, col), _) = self
                .possibilities
                .indexed_iter()
                .filter(|((r, c), bs)| self.mask[(*r, *c)] && bs.count_ones(..) > 1)
                .min_by_key(|(_, bs)| bs.count_ones(..))
                .expect("should have at least one unfixed cell");

            // the vector of possible tiles and their weights
            let opts = self.possibilities[(row, col)].ones().collect::<Vec<_>>();
            let weights = opts
                .iter()
                .map(|&t| self.tileset.frequency(t))
                .collect::<Vec<_>>();

            // pick one at random
            let dist = WeightedIndex::new(&weights).map_err(|e| anyhow!(e))?;
            let pick = opts[dist.sample(rng)];

            // collapse that cell to exactly 'pick'
            let mut bs = FixedBitSet::with_capacity(self.tileset.len());
            bs.insert(pick);
            self.possibilities[(row, col)] = bs;

            pb.inc(1);

            // propagate constraints again
            self.ac3()?;
        }

        pb.finish();

        // 3) Build the final map (should never fail)
        let tiles = self
            .possibilities
            .iter()
            .map(|bs| bs.ones().next().expect("each cell has exactly one choice"))
            .collect::<Vec<_>>();

        let arr = Array2::from_shape_vec(
            (height, width),
            tiles.into_iter().map(Tile::Fixed).collect(),
        )
        .map_err(|e: ndarray::ShapeError| anyhow!(e))?;

        Ok(Map::new(arr))
    }
}
