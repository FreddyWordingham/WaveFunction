use wave_function::TileSet;

const TILE_SIZE: usize = 1;
const BORDER_SIZE: usize = 1;

/// Read command line arguments.
fn read_inputs() -> String {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <tile_dir>", args[0]);
        std::process::exit(1);
    }
    let tile_dir = &args[1];

    tile_dir.to_string()
}

fn main() {
    let tile_dir = read_inputs();
    let tile_set =
        TileSet::load(TILE_SIZE, BORDER_SIZE, &tile_dir).expect("Failed to load tile set.");
    println!("Loaded {} tiles.", tile_set.num_tiles());

    let rule_set = tile_set.generate_rules();
    println!("{:?}", rule_set);
}
