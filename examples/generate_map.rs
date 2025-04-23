use clap::{Parser, ValueEnum};
use photo::ImageRGBA;
use rand::rng;
use std::{num::ParseIntError, path::PathBuf, str::FromStr};
use wave_function::{Map, Tileset, WaveFunctionBacktracking, WaveFunctionFast};

/// Only these three algorithms allowed
#[derive(ValueEnum, Debug, Clone)]
enum Algorithm {
    Fast,
    Backtracking,
}

/// Holds “NxM” and parses into two usize fields
#[derive(Debug, Clone)]
struct MapSize {
    width: usize,
    height: usize,
}

impl FromStr for MapSize {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let mut parts = s.split('x');
        let w = parts
            .next()
            .ok_or("missing width")?
            .parse()
            .map_err(|e: ParseIntError| e.to_string())?;
        let h = parts
            .next()
            .ok_or("missing height")?
            .parse()
            .map_err(|e: ParseIntError| e.to_string())?;
        if parts.next().is_some() {
            return Err("too many parts".into());
        }
        Ok(MapSize {
            width: w,
            height: h,
        })
    }
}

/// Image processing configuration.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Config {
    #[arg(short, long)]
    input_tileset: PathBuf,

    #[arg(short, long)]
    output_filepath: PathBuf,

    #[arg(short, long)]
    algorithm: Algorithm,

    #[arg(short, long)]
    map_size: MapSize,

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
            .zip(tileset.rules().frequencies())
            .enumerate()
            .map(|(i, (tile, frequency))| (tile, format!("{} ({})", i, frequency)))
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
        println!("Algorithm         : {:?}", config.algorithm);
        println!(
            "Map size          : {}x{}",
            config.map_size.width, config.map_size.height
        );
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
    }

    let tileset = Tileset::load(config.tile_size, config.border_size, &config.input_tileset);
    if config.verbose {
        println!("Number of tiles   : {}", tileset.len());
        print_tileset_images(&tileset);
    }

    let template = Map::empty((config.map_size.width, config.map_size.height));
    // template.set((0, 0), Cell::Fixed(1));

    // for i in 10..20 {
    //     for j in 10..20 {
    //         template.set((i, j), Cell::Fixed(131));
    //     }
    // }

    let mut rng = rng();

    let map = match config.algorithm {
        Algorithm::Fast => template
            .collapse::<WaveFunctionFast>(tileset.rules(), &mut rng)
            .expect("Failed to collapse map"),
        Algorithm::Backtracking => template
            .collapse::<WaveFunctionBacktracking>(tileset.rules(), &mut rng)
            .expect("Failed to collapse map"),
    };

    println!("{}", map);

    let img = map.render(&tileset);
    img.save(&config.output_filepath)
        .expect("Failed to save image");
}
