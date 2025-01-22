#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

"""
This manages a git sparse checkout in a Rust project environment.
It dynamically updates the checked-out directories based on a configuration file,
ignores specific files from being tracked by git, and adjusts the project's Cargo.toml
to match the sparse checkout state.

Usage:

  0. Run the script from an existing git checkout to create a new git worktree as a sparse checkout.
  1. Edit your .sparse file to include the set of directories you wish to check out, or
     use the `auto` subcommand to generate this file automatically based on which files
     you have edited since the merge base of the current commit and origin/main.
  2. Run the script to update the sparse checkout, and modify Cargo.toml as needed.
  3. Always re-run the script after modifying your .sparse file.

When in a sparse checkout, changes to Cargo.toml and Cargo.lock are ignored by git. If you need
to edit these files from within a sparse checkout, use the `reset` subcommand to un-ignore them.
Then edit them, check in the changes, and run this script again.

If you need to rebase or checkout a different branch, you can use the `git` subcommand to run
git commands after resetting the index. For example:

  $ ./sparse-checkout.py git rebase origin/main

After git completes successfully, it will re-run the script to update the sparse checkout.
If git does not complete successfully (e.g. rebase conflicts), you will need to manually
resolve the conflicts and re-run the script (with no arguments) to update the sparse checkout.
"""

import os
import subprocess
import sys

try:
    import toml
except ImportError:
    print("This script requires the 'toml' Python package. Install via: pip install toml")
    sys.exit(1)


def read_sparse_config(sparse_file=".sparse"):
    """
    Read the .sparse file and return a list of crates (paths) that should be included.
    """
    if os.path.isfile(sparse_file):
        crates = []
        with open(sparse_file, "r") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    crates.append(strip_trailing_slash(line))
        # throw error if no crates are found
        if not crates:
            print(f"No crates found in {sparse_file}. Exiting.")
            sys.exit(1)

        # sui-benchmark tests use sui-surfer which requires move sources to be checked out
        if "crates/sui-surfer" not in crates and "crates/sui-benchmark" in crates:
            crates.append("crates/sui-surfer")

        return crates
    else:
        return None


def update_git_sparse_checkout(crates_to_checkout):
    """
    Initialize or update the git sparse-checkout to include only the given crates.
    """

    # You can add any default directories you always want checked out here
    default_directories = ["scripts", ".cargo", ".changeset", ".config", ".github", "examples"]

    # if we don't have sui-framework, we probably need to add the move sources in order for tests
    # to run
    if "crates/sui-framework" not in crates_to_checkout:
        default_directories.append("crates/sui-framework/packages")

    # 1) Initialize sparse checkout (if not already).
    subprocess.check_call(["git", "sparse-checkout", "init", "--cone"])

    # 2) Set the paths we actually want to check out.
    cmd = ["git", "sparse-checkout", "set"] + crates_to_checkout + default_directories
    subprocess.check_call(cmd)

    # 3) run git checkout to refresh the sparse checkout
    subprocess.check_call(["git", "checkout"])

def load_cargo_toml(cargo_toml_path="Cargo.toml"):
    """
    Load the Cargo.toml file (from git, not the filesystem) and return the parsed TOML data.
    """

    try:
        cargo_toml_content = subprocess.check_output(["git", "show", f"HEAD:{cargo_toml_path}"]).decode()
        cargo_data = toml.loads(cargo_toml_content)
        return cargo_data
    except subprocess.CalledProcessError:
        print(f"Could not retrieve {cargo_toml_path} from git. Exiting.")
        sys.exit(1)

def strip_trailing_slash(s):
    if s is None:
        return None
    return s[:-1] if s.endswith("/") else s

def get_path(dep_spec):
    if not isinstance(dep_spec, dict):
        return None
    return strip_trailing_slash(dep_spec.get("path"))

def modify_cargo_toml(crates_to_checkout, cargo_toml_path="Cargo.toml"):
    """
    Remove crates not in crates_to_checkout from Cargo.toml [workspace.members].
    Then, for each missing crate:
      - Locate a matching dependency in [workspace.dependencies] that has path=<crate_path>.
      - Convert it from path-based to git-based, preserving the original dependency name (TOML key).
      - If no matching dependency is found, create a new entry with a fallback name.
    """
    # Load the Cargo.toml
    if not os.path.isfile(cargo_toml_path):
        print(f"Could not find {cargo_toml_path}. Exiting.")
        sys.exit(1)

    cargo_data = load_cargo_toml(cargo_toml_path)

    # Make sure we have workspace.members in the top-level Cargo.toml
    workspace = cargo_data.setdefault("workspace", {})
    members = workspace.get("members", [])
    excluded = workspace.get("exclude", [])

    all_directories = members + excluded

    # Determine which crates are missing from .sparse
    missing_crates = [m for m in all_directories if m not in crates_to_checkout]
    kept_members = [m for m in members if m in crates_to_checkout]

    # all kept crates including the ones in exclude
    kept_crates = [m for m in all_directories if m in crates_to_checkout]

    # Update workspace.members
    cargo_data["workspace"]["members"] = kept_members

    # Get the merge base of the current commit and origin/main
    commit_sha = subprocess.check_output(["git", "merge-base", "HEAD", "origin/main"]).decode().strip()

    # Get the git remote URL (assuming 'origin' is the correct remote)
    try:
        repo_url = subprocess.check_output(["git", "remote", "get-url", "origin"]).decode().strip()
        # Convert SSH URL (git@...) to HTTPS if desired
        if repo_url.startswith("git@"):
            # Example conversion: git@github.com:User/Repo.git -> https://github.com/User/Repo.git
            repo_url = repo_url.replace(":", "/").replace("git@", "https://")
    except subprocess.CalledProcessError:
        # If there's no 'origin', handle as you see fit
        repo_url = "https://unknown-repo-url"

    # Ensure [workspace.dependencies] is a dict
    workspace_deps = cargo_data["workspace"].setdefault("dependencies", {})


    # now find the names of every checked-out crate that are specified in workspace.dependencies
    kept_dep_names = [dep_name for dep_name in workspace_deps if get_path(workspace_deps[dep_name]) in kept_crates]
    patch = cargo_data.setdefault("patch", {})
    for dep_name in kept_dep_names:
        path = get_path(workspace_deps[dep_name])
        assert path is not None
        patch_section = patch.setdefault(f"{repo_url}", {})[dep_name] = { "path": path }

    # For each missing crate, we want to:
    # 1) Find a dependency in [workspace.dependencies] whose 'path' == crate_path.
    # 2) Replace that 'path' dep with a 'git' + 'rev' dep, preserving the key.
    for crate_path in missing_crates:
        matched_dep = False

        for dep_name, dep_spec in workspace_deps.items():
            # If dep_spec is not a dict, skip
            if not isinstance(dep_spec, dict):
                continue

            if strip_trailing_slash(dep_spec.get("path")) == crate_path:
                del dep_spec["path"]
                dep_spec["git"] = repo_url
                dep_spec["rev"] = commit_sha
                matched_dep = True
                break

    # Write updated Cargo.toml back
    with open(cargo_toml_path, "w") as f:
        toml.dump(cargo_data, f)

    print("Successfully updated Cargo.toml")

def get_ignored_files():
    ignored_files = subprocess.check_output(["git", "ls-files", "-v"]).decode().split("\n")
    ignored_files = [line.split(" ")[1] for line in ignored_files if line.startswith("h")]
    return ignored_files

def ignore_cargo_changes():
    """
    Ignore changes to Cargo.toml and Cargo.lock with the --assume-unchanged flag.
    """

    ignored_files = get_ignored_files()

    # ignored files should include only Cargo.toml and Cargo.lock
    if "Cargo.toml" not in ignored_files:
        print("Ignoring changes to Cargo.toml");
        subprocess.check_call(["git", "update-index", "--assume-unchanged", "Cargo.toml"])
    else:
        ignored_files.remove("Cargo.toml")

    if "Cargo.lock" not in ignored_files:
        print("Ignoring changes to Cargo.lock");
        subprocess.check_call(["git", "update-index", "--assume-unchanged", "Cargo.lock"])
    else:
        ignored_files.remove("Cargo.lock")

    # un-ignore any remaining files
    for file in ignored_files:
        print(f"Un-ignoring {file}")
        subprocess.check_call(["git", "update-index", "--no-assume-unchanged", file])

def create_sparse_checkout_worktree():
    # if .sparse is not found, offer to create a new sparse worktree
    print("No crates found in .sparse (or file not present).")
    print("Would you like to create a new sparse worktree? (Y/n)")
    choice = input().lower()
    if choice == "y" or choice == "":
        # move to git repo root
        os.chdir(subprocess.check_output(["git", "rev-parse", "--show-toplevel"]).decode().strip())
        # get basename of current directory
        dir = os.path.basename(os.getcwd())
        sparse_dir = f"../{dir}-sparse"

        # ask if they would like to use this name or a different one
        print(f"Would you like to use the directory name '{sparse_dir}' for the sparse worktree? (Y/n)")
        choice = input().lower()
        if choice == "n":
            print("Enter the name for the sparse worktree:")
            sparse_dir = input()
            # add ../ if not already present
            if not sparse_dir.startswith("../"):
                sparse_dir = f"../{sparse_dir}"

        print(f"Creating a new sparse worktree at {sparse_dir}")
        subprocess.check_call(["git", "worktree", "add", "--no-checkout", sparse_dir, "main"])

        # move to the sparse worktree
        os.chdir(sparse_dir)

        # now launch $EDITOR to configure the .sparse file. The default contents of .sparse
        # are `crates/sui-core`. First, write the defaults
        with open(".sparse", "w") as f:
            f.write("# Directories to include in the sparse checkout\n")
            f.write("crates/sui-core\n")
        # now launch $EDITOR
        subprocess.check_call([os.getenv("EDITOR", "vi"), ".sparse"])
    else:
        print("Exiting.")
        sys.exit(0)
    crates_to_checkout = read_sparse_config(".sparse")
    return crates_to_checkout

def reset_index():
    ignored_files = get_ignored_files()

    # check that Cargo.toml and Cargo.lock are ignored
    if "Cargo.toml" not in ignored_files or "Cargo.lock" not in ignored_files:
        print("Cargo.toml and/or Cargo.lock are not ignored. Reset them manually or check in your changes")
        sys.exit(1)

    subprocess.check_call(["git", "checkout", "Cargo.toml", "Cargo.lock"])

def auto_update_config():

    # get the list of files that have changed between the current commit and the merge base.
    # Use this to select the directories from the workspace that should be included in the sparse checkout.

    # Get the merge base of the current commit and origin/main
    commit_sha = subprocess.check_output(["git", "merge-base", "HEAD", "origin/main"]).decode().strip()

    # Get the list of files that have changed between the current commit and the merge base
    changed_files = subprocess.check_output(["git", "diff", "--name-only", commit_sha]).decode().split("\n")

    cargo_data = load_cargo_toml("Cargo.toml")

    # for every directory in workspace.members and exclude, check if it is a prefix of some changed file.
    # If it is, add it to the list of directories to checkout.
    directories_to_checkout = []

    for directory in cargo_data["workspace"]["members"] + cargo_data["workspace"].get("exclude", []):
        for file in changed_files:
            if file.startswith(directory):
                directories_to_checkout.append(directory)
                break

    # unique-ify the list
    directories_to_checkout = list(set(directories_to_checkout))

    # write the list of directories to .sparse
    with open(".sparse", "w") as f:
        f.write("# Directories to include in the sparse checkout\n")
        for directory in directories_to_checkout:
            f.write(f"{directory}\n")

def main():
    # if given the `reset` command, reset changes to Cargo.lock and Cargo.toml
    if len(sys.argv) > 1 and sys.argv[1] == "reset":
        reset_index()
        sys.exit(0)

    if len(sys.argv) > 1 and sys.argv[1] == "auto":
        auto_update_config()
        sys.exit(0)

    if len(sys.argv) > 1 and sys.argv[1] == "git":
        # check if there are any ignored files
        if get_ignored_files():
          reset_index()

        # run the git command
        subprocess.check_call(sys.argv[1:])
        # fall through to re-generate the Cargo.toml changes

    # 1. Read the crates to include from .sparse
    crates_to_checkout = read_sparse_config(".sparse")
    if crates_to_checkout is None:
        crates_to_checkout = create_sparse_checkout_worktree()
        assert crates_to_checkout is not None

    # 2. Update git sparse checkout
    update_git_sparse_checkout(crates_to_checkout)

    # 3. Ignore changes to Cargo.toml and Cargo.lock
    ignore_cargo_changes()

    # 4. Modify Cargo.toml
    modify_cargo_toml(crates_to_checkout, "Cargo.toml")


if __name__ == "__main__":
    main()
