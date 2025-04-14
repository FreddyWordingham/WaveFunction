//! # `WaveFunction`
//!
//! `WaveFunction` is a library for procedurally generating 2D maps.

// #![deny(warnings)]
#![deny(missing_docs)]
// #![deny(unused)]
// #![deny(dead_code)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

// mod map;
// mod rules;
mod tile;
mod tileset;
// mod wave_function;

// pub use map::Map;
// pub use rules::Rules;
pub use tile::Tile;
pub use tileset::Tileset;
// pub use wave_function::WaveFunction;
