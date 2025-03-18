use serde::{Deserialize, Serialize};

// enum Direction {
//     North,
//     East,
//     South,
//     West,
// }

// impl Direction {
//     fn index(&self) -> usize {
//         match self {
//             Self::North => 0,
//             Self::East => 1,
//             Self::South => 2,
//             Self::West => 3,
//         }
//     }
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct RuleSet {
    rules: Vec<[Vec<usize>; 4]>,
}

impl RuleSet {
    pub fn new(rules: Vec<[Vec<usize>; 4]>) -> Self {
        let num_tiles = rules.len();

        for rule in &rules {
            for adjacent_rules in rule {
                assert!(adjacent_rules.iter().all(|&tile| tile < num_tiles));
            }
        }

        Self { rules }
    }

    // Load a YAML file as a struct.
    pub fn load(filepath: &str) -> Self {
        let yaml = std::fs::read_to_string(filepath).unwrap();
        serde_yaml::from_str(&yaml).unwrap()
    }

    // Save the struct as a YAML file.
    pub fn save(&self, filepath: &str) -> Result<(), std::io::Error> {
        let yaml = serde_yaml::to_string(&self).unwrap();
        std::fs::write(filepath, yaml)
    }
}
