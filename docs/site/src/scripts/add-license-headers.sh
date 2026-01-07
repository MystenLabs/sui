# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

SHARED_DIR="../shared"
LICENSE_FILE="./license-header.txt"  # Each repo has its own license header file

# Check if license header file exists
if [ ! -f "$LICENSE_FILE" ]; then
  echo "Error: $LICENSE_FILE not found in repo root"
  exit 1
fi

LICENSE_HEADER=$(cat "$LICENSE_FILE")

# Function to add/replace header in a file
add_header() {
  local file="$1"
  local ext="${file##*.}"
  local temp_file=$(mktemp)
  
  # Determine comment style based on extension
  case "$ext" in
    js|jsx|ts|tsx|mjs)
      # Check if file already has a license header (starts with /*)
      if head -1 "$file" | grep -q "^/\*"; then
        # Remove existing header (everything up to and including */)
        sed -n '/\*\//,$p' "$file" | tail -n +2 > "$temp_file"
      else
        cat "$file" > "$temp_file"
      fi
      # Add new header
      echo "/*
$LICENSE_HEADER
*/" > "$file"
      cat "$temp_file" >> "$file"
      ;;
    css)
      # Same as JS
      if head -1 "$file" | grep -q "^/\*"; then
        sed -n '/\*\//,$p' "$file" | tail -n +2 > "$temp_file"
      else
        cat "$file" > "$temp_file"
      fi
      echo "/*
$LICENSE_HEADER
*/" > "$file"
      cat "$temp_file" >> "$file"
      ;;
  esac
  
  rm -f "$temp_file"
}

export -f add_header
export LICENSE_HEADER

# Find all relevant files and add headers
find "$SHARED_DIR" -type f \( -name "*.js" -o -name "*.jsx" -o -name "*.ts" -o -name "*.tsx" -o -name "*.css" -o -name "*.mjs" \) | while read file; do
  echo "Processing: $file"
  add_header "$file"
done

echo "License headers updated!"