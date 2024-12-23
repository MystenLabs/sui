# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
set -x

# install google protobuf definitions locally
if [[ ! -d googleapis ]]; then
  git clone https://github.com/googleapis/googleapis.git
fi

cargo run

# remove empty docstrings to satisfy `cargo test --doc` run
for file_name in google.api.rs google.bigtable.v2.rs; do
  file="../src/bigtable/proto/${file_name}"
  awk '!/^[[:space:]]*\/\/\/\s*$/ { print }' "$file" > tmp && mv tmp "$file"
done
