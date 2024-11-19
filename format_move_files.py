#!/usr/bin/env python3

import os
import subprocess
from pathlib import Path
from typing import Tuple, List


def run_git_diff(file_path: Path) -> bool:
    """
    Check if file has git differences.
    
    Args:
        file_path: Path to the file to check
        
    Returns:
        True if file has differences, False otherwise
    """
    result = subprocess.run(
        ["git", "diff", "--exit-code", str(file_path)],
        capture_output=True,
        text=True
    )
    return result.returncode != 0


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


def save_output(base_path: Path, content: str) -> None:
    """
    Save error content to a .stderr file next to the original file.
    
    Args:
        base_path: Original Move file path
        content: Error content to write
    """
    if content:  # Only write if there's content
        output_path = base_path.with_suffix(f"{base_path.suffix}.stderr")
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

    total_files = 0
    files_with_diffs: List[Path] = []
    files_with_errors: List[Path] = []

    for move_file in root_path.rglob("*.move"):
        total_files += 1
        print(f"Processing: {move_file}")
        
        stdout, stderr = run_prettier(move_file)
        
        # Write formatted content back to original file
        if stdout:
            move_file.write_text(stdout)
            if run_git_diff(move_file):
                files_with_diffs.append(move_file)
        
        # Save stderr to adjacent file if there were errors
        if stderr:
            files_with_errors.append(move_file)
            save_output(move_file, stderr)

    # Print summary
    print("\nSummary:")
    print(f"Total files processed: {total_files}")
    print(f"Files with formatting changes: {len(files_with_diffs)}")
    print(f"Files with errors: {len(files_with_errors)}")
    
    if files_with_diffs:
        print("\nFiles with formatting changes:")
        for file in files_with_diffs:
            print(f"  - {file}")
    
    if files_with_errors:
        print("\nFiles with errors:")
        for file in files_with_errors:
            print(f"  - {file}")


def main():
    tests_dir = "external-crates/move/crates/move-compiler/tests"
    process_move_files(tests_dir)


if __name__ == "__main__":
    main()