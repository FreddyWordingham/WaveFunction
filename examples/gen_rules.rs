use wave_function::TileSet;

const TILE_SIZE: usize = 1;
const BORDER_SIZE: usize = 1;

/// Read command line arguments.
fn read_inputs() -> (String, String) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_image> <output_filepath>", args[0]);
        std::process::exit(1);
    }
    let example_image_filepath = &args[1];
    let output_filepath = &args[2];

    (
        example_image_filepath.to_string(),
        output_filepath.to_string(),
    )
}

fn main() {
    let (tile_dir, output_filepath) = read_inputs();
    let tile_set =
        TileSet::load(TILE_SIZE, BORDER_SIZE, &tile_dir).expect("Failed to load tile set.");
    println!("Loaded {} tiles.", tile_set.num_tiles());

    let rule_set = tile_set.generate_rules();
    rule_set
        .save(&output_filepath)
        .expect("Failed to save rule set.");
}
