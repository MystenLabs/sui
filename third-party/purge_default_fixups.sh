#!/usr/bin/env bash

# Define the directory to search
base_dir="fixups"

# Define the pattern to match in fixups.toml
pattern=$(cat <<'EOF'
omit_targets = []
extra_srcs = []
omit_srcs = []
rustc_flags = []
cfgs = []
features = []
omit_features = []
extra_deps = []
omit_deps = []
cargo_env = false
EOF
)

# Iterate over each subdirectory in the base directory
find "$base_dir" -type f -name "fixups.toml" | while IFS= read -r toml_file; do
    # Read the contents of the fixups.toml file
    file_contents=$(cat "$toml_file")

    # Check if the file contents match the pattern
    if [[ "$file_contents" == "$pattern"* ]]; then
        # Get the directory containing the fixups.toml file
        dir_to_remove=$(dirname "$toml_file")

        # Remove the directory and its contents
        echo "Removing directory: $dir_to_remove"
        rm -rf "$dir_to_remove"
    fi
done

