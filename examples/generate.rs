use wave_function::{RuleSet, WaveFunction};

/// Read command line arguments.
fn read_inputs() -> (String, [usize; 2]) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <ruleset_filepath> <map_resolution>", args[0]);
        std::process::exit(1);
    }
    let tile_dir = &args[1];
    let resolution = args[2]
        .split('x')
        .map(|s| s.parse().expect("Failed to parse resolution"))
        .collect::<Vec<usize>>();

    (tile_dir.to_string(), (resolution[1], resolution[0]).into())
}

fn main() {
    let (ruleset_filepath, map_resolution) = read_inputs();
    let ruleset = RuleSet::load(&ruleset_filepath).expect("Failed to load rule set.");
    println!("Loaded {} rules.", ruleset.num_rules());

    let mut wf = WaveFunction::new(&ruleset, map_resolution);
    wf.ac3().expect("AC3 failed.");
    wf.set_tile(3, 3, 1).expect("Failed to set tile.");
    println!("AC3 completed successfully.");

    wf.collapse().expect("Failed to collapse.");
    let map = wf.generate_map();
    println!("{:?}", map);
}
