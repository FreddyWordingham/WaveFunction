use std::fmt::{Display, Formatter};

const CELL_IGNORE: &str = "!";
const CELL_WILDCARD: &str = "*";

#[derive(Clone, PartialEq)]
pub enum Cell {
    Ignore,
    Wildcard,
    Fixed(usize),
}

impl Display for Cell {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Cell::Ignore => write!(f, "{}", CELL_IGNORE),
            Cell::Wildcard => write!(f, "{}", CELL_WILDCARD),
            Cell::Fixed(index) => write!(f, "{}", index),
        }
    }
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        match s {
            "!" => Cell::Ignore,
            "*" => Cell::Wildcard,
            _ => {
                if let Ok(index) = s.parse::<usize>() {
                    Cell::Fixed(index)
                } else {
                    panic!("Invalid cell string: {}", s)
                }
            }
        }
    }
}
