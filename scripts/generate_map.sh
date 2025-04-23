cargo run --release --example generate_map -- \
    --input-tileset ./output/tiles/tiles.txt \
    --output-filepath ./output/tiles/map.png \
    --algorithm backtracking \
    --map-size 200x200 \
    --tile-size 3 \
    --border-size 1 \
    -v
