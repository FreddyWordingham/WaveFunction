#!/usr/bin/env bash

shopt -s globstar

for file in ./src/**/*.rs; do
  # only process regular files
  [[ -f "$file" ]] || continue

  echo "File: $file"
  echo '```rust'
  cat "$file"
  echo '```'
  echo
done
