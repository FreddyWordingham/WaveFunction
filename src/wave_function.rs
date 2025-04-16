use crate::{Map, Tileset};

pub struct WaveFunction {}

impl WaveFunction {
    pub fn collapse(map: Map, tileset: &Tileset) -> Map {
        debug_assert!(
            map.max_index().map_or(true, |index| index < tileset.len()),
            "Tile index out of bounds for tileset"
        );
        unimplemented!()
    }
}
