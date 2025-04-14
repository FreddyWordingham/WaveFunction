use clap::Parser;
use std::path::PathBuf;

/// Image processing configuration.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Config {
    #[arg(short, long)]
    input_image: PathBuf,

    #[arg(short, long)]
    output_dir: PathBuf,

    #[arg(short, long)]
    tile_size: usize,

    #[arg(short, long)]
    border_size: usize,

    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    println!("Cutting tiles...");
}
