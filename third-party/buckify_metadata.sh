#!/usr/bin/env bash
# this script will fetch cargo data at the buck root and write out a metadata file
# that we depend on for the buckify dep tool
# example use: ./buckify_metadata.sh @filter_platforms

# Function to read arguments from a file
read_args_from_file() {
    local file="$1"
    local args=()
    while IFS= read -r line || [ -n "$line" ]; do
        # Split the line into separate arguments
        args+=($line)
    done < "$file"
    # Output each argument separately
    printf "%s\n" "${args[@]}"
}

# Parse command line arguments
parsed_args=()
while (( "$#" )); do
    if [[ $1 == @* ]]; then
        file="${1:1}"
        # Read arguments from file and append them to parsed_args array
        args_from_file=($(read_args_from_file "$file"))
        parsed_args+=("${args_from_file[@]}")
    else
        parsed_args+=("$1")
    fi
    shift
done

main() {
    cd ..
    cargo fetch
    cargo metadata --frozen --locked --offline --format-version 1 ${parsed_args[@]} > third-party/cargo_metadata.json
    cd - > /dev/null
}

main
