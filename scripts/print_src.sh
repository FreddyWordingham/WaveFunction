#!/usr/bin/env bash

for file in ./src/*; do
  # print the filename
  echo "File: $file"

  # open a Rust code fence, cat the file, then close it
  echo '```rust'
  cat "$file"
  echo '```'

  # separator (optional)
  echo
done
