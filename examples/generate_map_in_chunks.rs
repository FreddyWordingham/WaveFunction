use clap::{Parser, ValueEnum};
use ndarray::Array2;
use photo::{Direction, ImageRGBA};
use rand::{Rng, rng};
use std::{num::ParseIntError, path::PathBuf, str::FromStr};
use wave_function::{Map, Rules, Tileset, WaveFunctionBacktracking, WaveFunctionFast};

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
            "Number of chunks  : {}x{}",
            config.num_chunks.width, config.num_chunks.height
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

    // Initialize array of empty chunks with valid dimensions
    let mut chunks = Array2::from_elem(
        (config.num_chunks.height, config.num_chunks.width),
        Map::empty((config.chunk_size.width, config.chunk_size.height)),
    );

    // Define a function to collapse a chunk based on the selected algorithm
    fn collapse_map<R: rand::Rng>(
        map: Map,
        rules: &Rules,
        rng: &mut R,
        algorithm: &Algorithm,
    ) -> Map {
        match algorithm {
            Algorithm::Fast => map
                .collapse::<WaveFunctionFast>(rules, rng)
                .expect("Failed to collapse map"),
            Algorithm::Backtracking => map
                .collapse::<WaveFunctionBacktracking>(rules, rng)
                .expect("Failed to collapse map"),
        }
    }

    // Generate chunks in a deterministic order to ensure border consistency

    // First, generate all chunks independently
    for y in 0..config.num_chunks.height {
        for x in 0..config.num_chunks.width {
            let empty_map = Map::empty((config.chunk_size.width, config.chunk_size.height));
            chunks[(y, x)] = collapse_map(empty_map, tileset.rules(), &mut rng, &config.algorithm);

            if config.verbose {
                println!("Generated initial chunk at position ({}, {})", x, y);
            }
        }
    }

    // Process borders in a way that avoids borrow checker issues
    // We'll use a separate loop for each direction

    // Process North-South borders (rows)
    for y in 1..config.num_chunks.height {
        for x in 0..config.num_chunks.width {
            // Create a bordering chunk from the northern neighbor
            let border = chunks[(y - 1, x)].bordering_chunk(Direction::South, config.border_size);

            // Create a new map with the border constraints
            let mut new_map = Map::empty((config.chunk_size.width, config.chunk_size.height));
            new_map.set_shared_border(&border, Direction::North, config.border_size);

            // Collapse the map with these constraints and update the chunk
            chunks[(y, x)] = collapse_map(new_map, tileset.rules(), &mut rng, &config.algorithm);

            if config.verbose {
                println!("Processed North-South border at ({}, {})", x, y);
            }
        }
    }

    // Process West-East borders (columns)
    for x in 1..config.num_chunks.width {
        for y in 0..config.num_chunks.height {
            // Create a bordering chunk from the western neighbor
            let border = chunks[(y, x - 1)].bordering_chunk(Direction::East, config.border_size);

            // Create a new map with the border constraints
            let mut new_map = Map::empty((config.chunk_size.width, config.chunk_size.height));
            new_map.set_shared_border(&border, Direction::West, config.border_size);

            // Collapse the map with these constraints and update the chunk
            chunks[(y, x)] = collapse_map(new_map, tileset.rules(), &mut rng, &config.algorithm);

            if config.verbose {
                println!("Processed West-East border at ({}, {})", x, y);
            }
        }
    }

    // Render all chunks and merge into one image
    let imgs = chunks
        .mapv(|c| c.render(&tileset))
        .map(|img| img.interior(config.border_size / 2));

    // Create final image from tiles
    let img = ImageRGBA::from_tiles(&imgs);
    img.save(&config.output_filepath)
        .expect("Failed to save image");
}
