#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""
This script is used to analyze threadstalls. It will print out the most common
stack patterns and the most frequent interesting functions.
"""

import os
import re
from collections import Counter, defaultdict
from typing import List, Dict, Set

class ThreadDumpAnalyzer:
    def __init__(self):
        # Add interesting path prefixes as a class attribute
        self.interesting_prefixes = [
            'crates/',
            'consensus/',
            'sui-execution/',
            'external-crates/',
        ]

    def is_interesting(self, line: str) -> bool:
        # Check if line starts with any of the interesting prefixes
        return any(line.startswith(prefix) for prefix in self.interesting_prefixes)

    def parse_dump_file(self, file_path: str) -> List[List[str]]:
        stacks = []
        current_stack = []

        with open(file_path, 'r') as f:
            for line in f:
                line = line.strip()
                if line.startswith('Thread'):
                    if current_stack:
                        stacks.append(current_stack)
                    current_stack = []
                elif line and not line.startswith('---'):
                    if ' at ' in line:
                        func = line.split(' at ')[-1].strip()
                        current_stack.append(func)

        if current_stack:
            stacks.append(current_stack)

        return stacks

    def compress_stack(self, stack: List[str]) -> List[str]:
        """Compress stack to only interesting parts while maintaining context"""
        compressed = []
        last_was_skipped = False

        for frame in stack:
            if self.is_interesting(frame):
                compressed.append(frame)
                last_was_skipped = False
            else:
                if not last_was_skipped:
                    compressed.append("...")  # Add ellipsis to show skipped frames
                    last_was_skipped = True

        # Remove trailing ellipsis
        if compressed and compressed[-1] == "...":
            compressed.pop()

        return compressed

    def analyze_dumps(self, dump_dir: str) -> Dict:
        all_dumps_analysis = {
            'individual_dumps': [],
            'common_patterns': defaultdict(int),
            'frequent_functions': Counter()
        }

        dump_files = [f for f in os.listdir(dump_dir)]

        for dump_file in sorted(dump_files):
            file_path = os.path.join(dump_dir, dump_file)
            stacks = self.parse_dump_file(file_path)

            dump_analysis = {
                'file': dump_file,
                'compressed_stacks': []
            }

            for stack in stacks:
                compressed = self.compress_stack(stack)
                if compressed:
                    dump_analysis['compressed_stacks'].append(compressed)

                    # Track individual interesting functions
                    for frame in compressed:
                        if frame != "...":
                            all_dumps_analysis['frequent_functions'][frame] += 1

                    # Track stack patterns
                    stack_signature = ' -> '.join(compressed)
                    all_dumps_analysis['common_patterns'][stack_signature] += 1

            all_dumps_analysis['individual_dumps'].append(dump_analysis)

        return all_dumps_analysis

    def print_analysis(self, analysis: Dict):
        print("=== Individual Dump Analysis ===")
        for dump in analysis['individual_dumps']:
            print(f"\nFile: {dump['file']}")
            print("Compressed stacks:")

            for stack_num, stack in enumerate(dump['compressed_stacks'], 1):
                print(f"\nStack {stack_num}:")
                for frame in stack:
                    print(f"  {frame}")

        print("\n=== Most Common Stack Patterns ===")
        for pattern, count in sorted(analysis['common_patterns'].items(), key=lambda x: x[1], reverse=True)[:10]:
            print(f"\nOccurred {count} times:")
            for frame in pattern.split(' -> '):
                print(f"  {frame}")

        print("\n=== Most Frequent Functions ===")
        for func, count in analysis['frequent_functions'].most_common(20):
            print(f"{func}: {count} occurrences")

def main():
    analyzer = ThreadDumpAnalyzer()
    import sys
    if len(sys.argv) != 2:
        print("Usage: threadstall_analyzer.py <dump_directory>")
        sys.exit(1)
    analysis = analyzer.analyze_dumps(sys.argv[1])
    analyzer.print_analysis(analysis)

if __name__ == "__main__":
    main()
