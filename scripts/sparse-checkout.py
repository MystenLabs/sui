#!/usr/bin/env python3

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
    crates = []
    if os.path.isfile(sparse_file):
        with open(sparse_file, "r") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    crates.append(line)
    return crates


def update_git_sparse_checkout(crates_to_checkout):
    """
    Initialize or update the git sparse-checkout to include only the given crates.
    """

    # You can add any default directories you always want checked out here
    default_directories = ["scripts"]

    # 1) Initialize sparse checkout (if not already).
    subprocess.check_call(["git", "sparse-checkout", "init", "--cone"])

    # 2) Set the paths we actually want to check out.
    cmd = ["git", "sparse-checkout", "set"] + crates_to_checkout + default_directories
    subprocess.check_call(cmd)


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

    with open(cargo_toml_path, "r") as f:
        cargo_data = toml.load(f)

    # Make sure we have workspace.members in the top-level Cargo.toml
    workspace = cargo_data.setdefault("workspace", {})
    members = workspace.get("members", [])
    excluded = workspace.get("exclude", [])

    all_directories = members + excluded

    # Determine which crates are missing from .sparse
    missing_crates = [m for m in all_directories if m not in crates_to_checkout]
    kept_crates = [m for m in members if m in crates_to_checkout]

    # Update workspace.members
    cargo_data["workspace"]["members"] = kept_crates

    # Get the current commit SHA
    commit_sha = subprocess.check_output(["git", "rev-parse", "HEAD"]).decode().strip()

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

    # For each missing crate, we want to:
    # 1) Find a dependency in [workspace.dependencies] whose 'path' == crate_path.
    # 2) Replace that 'path' dep with a 'git' + 'rev' dep, preserving the key.
    # 3) If none is found, create a new dependency entry with a fallback name.
    for crate_path in missing_crates:
        matched_dep = False

        for dep_name, dep_spec in workspace_deps.items():
            # If dep_spec is not a dict, skip
            if not isinstance(dep_spec, dict):
                continue

            # If the 'path' matches, update it
            if dep_spec.get("path") == crate_path:
                del dep_spec["path"]
                dep_spec["git"] = repo_url
                dep_spec["rev"] = commit_sha
                matched_dep = True
                break

        if not matched_dep:
            # No existing dependency matched this path,
            # so create one with a fallback name derived from the directory.
            crate_name = os.path.basename(os.path.normpath(crate_path))
            workspace_deps[crate_name] = {
                "git": repo_url,
                "rev": commit_sha,
            }

    # Write updated Cargo.toml back
    with open(cargo_toml_path, "w") as f:
        toml.dump(cargo_data, f)

    print("Successfully updated Cargo.toml")


def main():
    # 1. Read the crates to include from .sparse
    crates_to_checkout = read_sparse_config(".sparse")
    if not crates_to_checkout:
        print("No crates found in .sparse (or file not present). Exiting.")
        sys.exit(0)

    # 2. Update git sparse checkout
    update_git_sparse_checkout(crates_to_checkout)

    # 3. Modify Cargo.toml
    modify_cargo_toml(crates_to_checkout, "Cargo.toml")


if __name__ == "__main__":
    main()
