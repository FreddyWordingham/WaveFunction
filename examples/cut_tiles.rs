use clap::Parser;
use photo::{ALL_TRANSFORMATIONS, ImageRGBA};
use std::path::PathBuf;
use wave_function::TilesetBuilder;

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

    #[arg(short = 's', long)]
    tile_size: usize,

    #[arg(short, long)]
    border_size: usize,

    #[arg(short = 't', long)]
    all_transformations: bool,

    #[clap(short, long)]
    verbose: bool,
}

/// Load the input image from the given path and display it if verbose mode is enabled.
fn load_input_image(config: &Config) -> ImageRGBA<u8> {
    let example_image = ImageRGBA::<u8>::load(&config.input_image).unwrap_or_else(|_| {
        panic!(
            "Failed to load example image: {}",
            config.input_image.display()
        )
    });

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

fn print_tileset_images(tileset_builder: &TilesetBuilder) {
    ImageRGBA::print_image_grid_with_caption(
        &tileset_builder
            .tiles()
            .iter()
            .map(|tile| (&tile.0, tile.1.to_string()))
            .collect::<Vec<_>>(),
        1,
    )
    .unwrap();
}

fn main() {
    let config = Config::parse();
    if config.verbose {
        println!("Input image       : {}", config.input_image.display());
        println!("Output directory  : {}", config.output_dir.display());
        println!("Overlap           : {}", config.overlap);
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
        println!("Transformations   : {}", config.all_transformations);
    }

    let input_image = load_input_image(&config);

    let transformations = if config.all_transformations {
        ALL_TRANSFORMATIONS.to_vec()
    } else {
        vec![photo::Transformation::Identity]
    };

    let tileset_builder = TilesetBuilder::new(config.tile_size, config.border_size).add_tiles(
        &input_image,
        config.overlap,
        &transformations,
    );
    if config.verbose {
        println!("Number of tiles   : {}", tileset_builder.len());
        print_tileset_images(&tileset_builder);
    }

    // Build the `Tileset` (calculate the adjacency rules).
    let tileset = tileset_builder.build();

    // Delete all files in the output directory.
    if config.output_dir.exists() {
        std::fs::remove_dir_all(&config.output_dir).unwrap_or_else(|_| {
            panic!(
                "Failed to remove output directory: {}",
                config.output_dir.display()
            )
        });
    }
    tileset
        .save(&config.output_dir)
        .expect("Failed to save tileset");
}
