use anyhow::Result;
use rand::Rng;

use crate::{Map, Rules};

pub trait WaveFunction {
    fn collapse(map: &Map, rules: &Rules, rng: &mut impl Rng) -> Result<Map>;
}
