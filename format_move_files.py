#!/usr/bin/env python3

import os
import subprocess
from pathlib import Path
from typing import Tuple


def run_prettier(file_path: Path) -> Tuple[str, str]:
    """
    Run prettier on a single Move file and return stdout and stderr.
    
    Args:
        file_path: Path to the Move file to format
        
    Returns:
        Tuple of (stdout, stderr) from prettier
    """
    try:
        result = subprocess.run(
            [
                "./node_modules/.bin/prettier",
                "--plugin=prettier-plugin-move",
                str(file_path)
            ],
            capture_output=True,
            text=True,
            check=True
        )
        return result.stdout, result.stderr
    except subprocess.CalledProcessError as e:
        return e.stdout, e.stderr


def save_output(base_path: Path, content: str, suffix: str) -> None:
    """
    Save content to a file with the given suffix next to the original file.
    
    Args:
        base_path: Original Move file path
        content: Content to write
        suffix: Suffix to append to the filename (e.g. '.stdout')
    """
    if content:  # Only write if there's content
        output_path = base_path.with_suffix(f"{base_path.suffix}{suffix}")
        output_path.write_text(content)


def process_move_files(root_dir: str) -> None:
    """
    Process all .move files in the given directory and its subdirectories.
    
    Args:
        root_dir: Root directory to start searching for Move files
    """
    root_path = Path(root_dir)
    
    if not root_path.exists():
        print(f"Error: Directory '{root_dir}' does not exist")
        return

    for move_file in root_path.rglob("*.move"):
        print(f"Processing: {move_file}")
        
        stdout, stderr = run_prettier(move_file)
        
        # Save stdout and stderr to adjacent files
        save_output(move_file, stdout, ".stdout")
        save_output(move_file, stderr, ".stderr")


def main():
    tests_dir = "external-crates/move/crates/move-compiler/tests"
    process_move_files(tests_dir)


if __name__ == "__main__":
    main()