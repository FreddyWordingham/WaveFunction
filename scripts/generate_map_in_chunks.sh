cargo run --release --example generate_map_in_chunks -- \
    --input-tileset ./output/tiles/tiles.txt \
    --output-filepath ./output/tiles/map.png \
    --algorithm backtracking \
    --chunk-size 10x10 \
    --num-chunks 2x1 \
    --tile-size 3 \
    --border-size 4 \
    -v
