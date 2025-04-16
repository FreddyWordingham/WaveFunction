use clap::Parser;
use photo::ImageRGBA;
use std::path::PathBuf;
use wave_function::Tileset;

/// Image processing configuration.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Config {
    #[arg(short = 'i', long)]
    tileset_dir: PathBuf,

    #[arg(short, long)]
    output_filepath: PathBuf,

    #[arg(short = 's', long)]
    tile_size: usize,

    #[arg(short, long)]
    border_size: usize,

    #[clap(short, long)]
    verbose: bool,
}

/// Print the images in the `Tileset` with their frequencies.
fn print_tileset_images(tileset: &Tileset) {
    let mut images = Vec::with_capacity(tileset.num_tiles());
    for tile in tileset.tiles() {
        images.push((tile.image(), format!("{}", tile.frequency())));
    }

    // Print out the of images in an array. Get pixel data with tile.image().data[[y, x]].
    ImageRGBA::print_image_grid_with_caption(&images, 1).unwrap();
}

fn main() {
    let config = Config::parse();
    if config.verbose {
        println!("Tileset directory : {}", config.tileset_dir.display());
        println!("Output filepath   : {}", config.output_filepath.display());
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
    }

    let tileset = Tileset::new(config.tile_size, config.border_size)
        .load(&config.tileset_dir)
        .expect("Failed to load tileset");
    if config.verbose {
        println!("Number of tiles   : {}", tileset.num_tiles());
        print_tileset_images(&tileset);
    }

    let rules = tileset.rules();
    if config.verbose {
        println!("{}", rules);
    }
    rules
        .save(&config.output_filepath)
        .expect("Failed to save rules");
}
