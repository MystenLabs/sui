#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import os
import re
import argparse

def replace_protocol_version_in_file(file_path, old_version, new_version, yes_to_all, dry_run):
    with open(file_path, 'r') as file:
        content = file.read()

    updated_content = re.sub(
        f"init --protocol-version {old_version} --",
        f"init --protocol-version {new_version} --",
        content
    )

    if content != updated_content:
        print(f"Found 'init --protocol-version {old_version}' in {file_path}")
        if dry_run:
            return
        if yes_to_all :
           with open(file_path, 'w') as file:
               file.write(updated_content)
           print(f"Updated {file_path}")
        else:
            confirm = input(f"Do you want to replace '--protocol-version {old_version}' with '--protocol-version {new_version}'? (yes/no): ").strip().lower()
            if confirm == 'yes' or confirm == 'y':
                with open(file_path, 'w') as file:
                    file.write(updated_content)
                print(f"Updated {file_path}")
            else:
                print(f"Skipped {file_path}")

def replace_protocol_version_in_repo(repo_path, old_version, new_version, yes_to_all, dry_run):
    for root, dirs, files in os.walk(repo_path):
        for file in files:
            if "sui-graphql-e2e-tests" in root.split(os.sep):
                if file.endswith('.move'):
                    file_path = os.path.join(root, file)
                    replace_protocol_version_in_file(file_path, old_version, new_version, yes_to_all, dry_run)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Replace protocol version in .move files.')
    parser.add_argument('--yes-to-all', action='store_true', help='Automatically say "yes to all" for all changes')
    parser.add_argument('--dry-run', action='store_true', help='List all files that will be updated without making any changes')
    args = parser.parse_args()

    repo_path = os.getcwd()
    old_version = input("Enter the old protocol version (XX): ")
    new_version = input("Enter the new protocol version (YY): ")
    replace_protocol_version_in_repo(repo_path, old_version, new_version, args.yes_to_all, args.dry_run)
    if not args.dry_run:
        print(f"Next step. Running `env UB=1 cargo nextest run --features pg_integration` in `crates/sui-graphql-e2e-tests` to update all the snapshots.")
