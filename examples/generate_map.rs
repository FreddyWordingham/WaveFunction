use clap::Parser;
use photo::ImageRGBA;
use rand::rng;
use std::path::PathBuf;
use wave_function::{Map, Tile, Tileset, WaveFunction};

/// Image processing configuration.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Config {
    #[arg(short, long)]
    input_tileset: PathBuf,

    #[arg(short, long)]
    output_filepath: PathBuf,

    #[arg(short = 's', long)]
    tile_size: usize,

    #[arg(short, long)]
    border_size: usize,

    #[clap(short, long)]
    verbose: bool,
}

fn print_tileset_images(tileset: &Tileset) {
    ImageRGBA::print_image_grid_with_caption(
        &tileset
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
        println!("Input image       : {}", config.input_tileset.display());
        println!("Output directory  : {}", config.output_filepath.display());
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
    }

    let tileset = Tileset::load(config.tile_size, config.border_size, &config.input_tileset);
    if config.verbose {
        println!("Number of tiles   : {}", tileset.len());
        print_tileset_images(&tileset);
    }

    let resolution = (200, 200);
    let mut template = Map::empty(resolution);
    template.set((0, 0), Tile::Fixed(1));
    template.set((9, 9), Tile::Ignore);

    let mut rng = rng();

    // retry loop
    let collapsed_map = (0..1000)
        .filter_map(|attempt| {
            let mut wf: WaveFunction = WaveFunction::new(&template, &tileset);
            match wf.collapse(&mut rng, &tileset) {
                Ok(map) => {
                    println!("WFC succeeded on attempt {}", attempt + 1);
                    Some(map)
                }
                Err(e) => {
                    eprintln!("WFC failed on attempt {}: {}", attempt + 1, e);
                    None
                }
            }
        })
        .next()
        .expect("All WFC attempts failed");

    let img = collapsed_map.render(&tileset);
    img.save(&config.output_filepath)
        .expect("Failed to save image");
}
