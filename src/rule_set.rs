use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub north: Vec<usize>,
    pub east: Vec<usize>,
    pub south: Vec<usize>,
    pub west: Vec<usize>,
}

impl Rule {
    pub fn new(north: Vec<usize>, east: Vec<usize>, south: Vec<usize>, west: Vec<usize>) -> Self {
        Self {
            north,
            east,
            south,
            west,
        }
    }

    pub fn num_rules(&self) -> usize {
        self.north.len() + self.east.len() + self.south.len() + self.west.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new(rules: Vec<Rule>) -> Self {
        let num_tiles = rules.len();

        // Check that each rule contains valid tile indices.
        for rule in &rules {
            for adjacent_tiles in &[&rule.north, &rule.east, &rule.south, &rule.west] {
                assert!(adjacent_tiles.iter().all(|&tile| tile < num_tiles));
            }
        }

        // Check that each rule is symmetric.
        for (i, rule) in rules.iter().enumerate() {
            for j in rule.north.iter() {
                assert!(rules[*j].south.contains(&i));
            }
            for j in rule.east.iter() {
                assert!(rules[*j].west.contains(&i));
            }
            for j in rule.south.iter() {
                assert!(rules[*j].north.contains(&i));
            }
            for j in rule.west.iter() {
                assert!(rules[*j].east.contains(&i));
            }
        }

        Self { rules }
    }

    // Load a YAML file as a struct.
    pub fn load(filepath: &str) -> Result<Self> {
        let yaml = std::fs::read_to_string(filepath)?;
        let parsed = serde_yaml::from_str(&yaml)?;
        Ok(parsed)
    }

    // Save the struct as a YAML file.
    pub fn save(&self, filepath: &str) -> Result<(), std::io::Error> {
        let yaml = serde_yaml::to_string(&self).unwrap();
        std::fs::write(filepath, yaml)
    }

    /// Get the rule for a given tile index.
    pub fn rule(&self, tile: usize) -> &Rule {
        &self.rules[tile]
    }

    /// Get the total number of tiles in the set.
    pub fn num_tiles(&self) -> usize {
        self.rules.len()
    }

    /// Get the total number of rules in the set.
    pub fn num_rules(&self) -> usize {
        self.rules
            .iter()
            .fold(0, |acc, rule| acc + rule.num_rules())
    }
}
