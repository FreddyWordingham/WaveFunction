use std::fmt::{Display, Formatter};

const TILE_IGNORE: &str = "!";
const TILE_WILDCARD: &str = "*";

#[derive(Clone, PartialEq)]
pub enum Tile {
    Ignore,
    Wildcard,
    Fixed(usize),
}

impl Display for Tile {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Tile::Ignore => write!(f, "{}", TILE_IGNORE),
            Tile::Wildcard => write!(f, "{}", TILE_WILDCARD),
            Tile::Fixed(index) => write!(f, "{}", index),
        }
    }
}

impl From<&str> for Tile {
    fn from(s: &str) -> Self {
        match s {
            "!" => Tile::Ignore,
            "*" => Tile::Wildcard,
            _ => {
                if let Ok(index) = s.parse::<usize>() {
                    Tile::Fixed(index)
                } else {
                    panic!("Invalid tile string: {}", s)
                }
            }
        }
    }
}
