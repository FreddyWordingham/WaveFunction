use photo::{ALL_TRANSFORMATIONS, ImageRGBA};
use wave_function::TileSet;

const TILE_SIZE: usize = 1;
const BORDER_SIZE: usize = 1;

/// Read command line arguments.
fn read_inputs() -> (String, String) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input_image> <output_dir>", args[0]);
        std::process::exit(1);
    }
    let example_image_filepath = &args[1];
    let output_dir = &args[2];

    (example_image_filepath.to_string(), output_dir.to_string())
}

fn main() {
    let (example_image_filepath, output_dir) = read_inputs();

    let example_map =
        ImageRGBA::<u8>::load(example_image_filepath).expect("Failed to load example map image.");
    println!("{}", example_map);

    let tile_set = TileSet::new(TILE_SIZE, BORDER_SIZE)
        .ingest(&example_map)
        .with_transformations(&ALL_TRANSFORMATIONS);
    println!("Num tiles: {}", tile_set.num_tiles());

    // Create the output directory if it does not exist, and wipe it if it does.
    if std::path::Path::new(&output_dir).exists() {
        std::fs::remove_dir_all(&output_dir).expect("Failed to remove output directory.");
    }
    std::fs::create_dir_all(&output_dir).expect("Failed to create output directory.");
    tile_set
        .save(&output_dir)
        .expect("Failed to save tile set.");
}
