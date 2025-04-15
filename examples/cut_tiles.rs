use clap::Parser;
use photo::{ALL_TRANSFORMATIONS, ImageRGBA};
use std::path::PathBuf;
use wave_function::Tileset;

/// Image processing configuration.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Config {
    #[arg(short, long)]
    input_image: PathBuf,

    #[arg(short, long)]
    output_dir: PathBuf,

    #[arg(short = 'l', long)]
    overlap: usize,

    #[arg(short, long)]
    tile_size: usize,

    #[arg(short, long)]
    border_size: usize,

    #[clap(short, long)]
    verbose: bool,
}

fn load_input_image(config: &Config) -> ImageRGBA<u8> {
    let example_image = ImageRGBA::<u8>::load(&config.input_image).expect(&format!(
        "Failed to load example image: {}",
        config.input_image.display()
    ));

    if config.verbose {
        println!(
            "Example size      : {}x{}",
            example_image.width(),
            example_image.height()
        );
        println!("{}", example_image);
    }

    example_image
}

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
        println!("Input image       : {}", config.input_image.display());
        println!("Output directory  : {}", config.output_dir.display());
        println!("Overlap           : {}", config.overlap);
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
    }

    let input_image = load_input_image(&config);

    // let transformation = ALL_TRANSFORMATIONS;
    let transformation = [photo::Transformation::Identity];

    let tileset = Tileset::new(config.tile_size, config.border_size).add_tiles(
        &input_image,
        config.overlap,
        &transformation,
    );
    if config.verbose {
        println!("Number of tiles   : {}", tileset.num_tiles());
        print_tileset_images(&tileset);
    }
}
