cargo run --release --example generate_map -- \
    --input-tileset ./output/tiles/tiles.txt \
    --output-filepath ./output/tiles/map.png \
    --algorithm fast \
    --map-size 50x50 \
    --tile-size 3 \
    --border-size 1 \
    -v
