#!/usr/bin/env python3
"""
Script to remove 'from_file_path' field from all JSON files in debug_info subdirectories.
"""

import json
import sys
from pathlib import Path


def remove_from_file_path(json_file: Path) -> bool:
    """
    Remove 'from_file_path' field from a JSON file.

    Returns True if the field was found and removed, False otherwise.
    """
    try:
        with open(json_file, 'r') as f:
            data = json.load(f)

        if 'from_file_path' in data:
            del data['from_file_path']

            with open(json_file, 'w') as f:
                json.dump(data, f, separators=(',', ':'))

            return True
        return False
    except Exception as e:
        print(f"Error processing {json_file}: {e}", file=sys.stderr)
        return False


def main():
    current_dir = Path.cwd()
    json_files_processed = 0
    files_modified = 0

    for debug_info_dir in current_dir.rglob('debug_info'):
        if debug_info_dir.is_dir():
            for json_file in debug_info_dir.rglob('*.json'):
                json_files_processed += 1
                if remove_from_file_path(json_file):
                    files_modified += 1
                    print(f"Removed 'from_file_path' from: {json_file.relative_to(current_dir)}")

    print(f"\nProcessed {json_files_processed} JSON files")
    print(f"Modified {files_modified} files")


if __name__ == '__main__':
    main()
