use clap::{Parser, ValueEnum};
use ndarray::Array2;
use photo::{Direction, ImageRGBA};
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
    chunk_size: MapSize,

    #[arg(short, long)]
    num_chunks: MapSize,

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
            "Chunk size        : {}x{}",
            config.chunk_size.width, config.chunk_size.height
        );
        println!(
            "Number of chunks       : {}x{}",
            config.chunk_size.width, config.chunk_size.height
        );
        println!("Tile size         : {}", config.tile_size);
        println!("Border size       : {}", config.border_size);
    }

    let tileset = Tileset::load(config.tile_size, config.border_size, &config.input_tileset);
    if config.verbose {
        println!("Number of tiles   : {}", tileset.len());
        print_tileset_images(&tileset);
    }

    let mut rng = rng();

    let mut chunks = Array2::from_elem(
        (config.num_chunks.width, config.num_chunks.height),
        Map::empty((config.chunk_size.width, config.chunk_size.height)),
    );

    chunks[(0, 0)] = match config.algorithm {
        Algorithm::Fast => Map::empty((config.chunk_size.width, config.chunk_size.height))
            .collapse::<WaveFunctionFast>(tileset.rules(), &mut rng)
            .expect("Failed to collapse map"),
        Algorithm::Backtracking => Map::empty((config.chunk_size.width, config.chunk_size.height))
            .collapse::<WaveFunctionBacktracking>(tileset.rules(), &mut rng)
            .expect("Failed to collapse map"),
    };
    println!("{}", chunks[(0, 0)]);

    chunks[(1, 0)] = chunks[(0, 0)].bordering_chunk(Direction::East, config.border_size);
    println!("{}", chunks[(1, 0)]);

    // let imgs = chunks
    //     .mapv(|c| c.render(&tileset))
    //     .map(|img| img.interior(1));
    // let img = ImageRGBA::from_tiles(&imgs);
    // img.save(&config.output_filepath)
    //     .expect("Failed to save image");
}
