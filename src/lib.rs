//! # `WaveFunction`
//!
//! `WaveFunction` is a library for procedurally generating 2D maps.

// #![deny(warnings)]
// #![deny(missing_docs)]
// #![deny(unused)]
// #![deny(dead_code)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(unreachable_code)]

mod map;
mod rules;
mod tile;
mod tileset;
mod tileset_builder;
mod wave_function;

pub use map::Map;
pub use rules::Rules;
pub use tile::Tile;
pub use tileset::Tileset;
pub use tileset_builder::TilesetBuilder;
pub use wave_function::WaveFunction;
