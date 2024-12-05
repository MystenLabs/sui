#!/usr/bin/env sh
# script to create missing fixup directories. generally not needed but sometimes reindeer misses things

# Check if exactly one argument is provided
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 folder name here"
    exit 1
fi

folder="$1"

mkdir fixups/$folder
touch fixups/$folder/fixups.toml
