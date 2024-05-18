#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
from os import chdir, remove
from pathlib import Path
import re
from shutil import which, rmtree
import subprocess
from sys import stderr, stdout
from typing import TextIO, Union


def parse_args():
    parser = argparse.ArgumentParser(
        prog="execution-layer",
    )

    subparsers = parser.add_subparsers(
        description="Tools for managing cuts of the execution-layer.",
    )

    cut = subparsers.add_parser(
        "cut",
        help=(
            "Create a new copy of execution-related crates, and add them to "
            "the workspace."
        ),
    )
    cut.set_defaults(cmd="Cut", do=do_cut)
    cut.add_argument(
        "feature",
        type=feature,
        help="The name of the new cut to make.",
    )
    cut.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the operations, without running them.",
    )

    generate_lib = subparsers.add_parser(
        "generate-lib",
        help=(
            "Generate `sui-execution/src/lib.rs` based on the current set of "
            "execution crates (i.e. without adding a new one).  Prints out "
            "what the contents of `lib.rs` would be without actually writing "
            "out to the file, if --dry-run flag is supplied."
        ),
    )
    generate_lib.set_defaults(cmd="Generating lib.rs", do=do_generate_lib)
    generate_lib.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the operations, without running them.",
    )

    merge = subparsers.add_parser(
        "merge",
        help="Apply the changes made to FEATURE since it was cut, to BASE.",
    )
    merge.set_defaults(cmd="Merge", do=do_merge)
    merge.add_argument("base", type=feature, help="The cut to merge into.")
    merge.add_argument("feature", type=feature, help="The cut to take from.")
    merge.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the patch file to apply to BASE without applying it.",
    )
    merge.add_argument(
        "-f",
        "--force",
        action="store_true",
        help=(
            "Merge regardless of warnings that the working directory is "
            "not clean.  If the merge fails at this point, it may be "
            "difficult to recover from."
        ),
    )

    patch = subparsers.add_parser(
        "patch",
        help=(
            "Print a patch file representing the changes made to FEATURE "
            "after the commit in which it was cut."
        ),
    )
    patch.set_defaults(do=do_patch)
    patch.add_argument(
        "feature",
        type=feature,
        help="The name of the cut to generate the patch for.",
    )

    rebase = subparsers.add_parser(
        "rebase",
        help=(
            "Rebase the changes to FEATURE against the current contents of "
            "'latest'."
        ),
    )
    rebase.set_defaults(cmd="Rebase", do=do_rebase)
    rebase.add_argument(
        "feature",
        type=feature,
        help="The name of the cut to rebase against 'latest'.",
    )
    rebase.add_argument(
        "-f",
        "--force",
        action="store_true",
        help=(
            "Rebase regardless of warnings that the working directory is "
            "not clean.  If the rebase fails at this point, it may be "
            "difficult to recover from."
        ),
    )

    return parser.parse_args()


def feature(f):
    if re.match(r"[a-z][a-zA-Z0-9_]*", f):
        return f
    else:
        raise argparse.ArgumentTypeError(f"Invalid feature name: '{f}'")


def do_cut(args):
    """Perform the actions of the 'cut' sub-command.
    Accepts the parsed command-line arguments as a parameter."""
    ensure_cut_binary()
    cmd = cut_command(args.feature)

    if args.dry_run:
        cmd.append("--dry-run")
        print(run(cmd))
        return

    impl_module = impl(args.feature)
    if impl_module.is_file():
        raise Exception(
            f"Impl for '{args.feature}' already exists at '{impl_module}'"
        )

    print("Cutting new release...", file=stderr)
    result = subprocess.run(cmd, stdout=stdout, stderr=stderr)

    if result.returncode != 0:
        print("Cut failed", file=stderr)
        exit(result.returncode)

    update_toml(args.feature, Path() / "sui-execution" / "Cargo.toml")
    generate_impls(args.feature, impl_module)

    with open(Path() / "sui-execution" / "src" / "lib.rs", mode="w") as lib:
        generate_lib(lib)


def do_generate_lib(args):
    if args.dry_run:
        generate_lib(stdout)
    else:
        lib_path = Path() / "sui-execution" / "src" / "lib.rs"
        with open(lib_path, mode="w") as lib:
            generate_lib(lib)

def do_merge(args):
    from_module = impl(args.feature)
    if not from_module.is_file():
        raise Exception(f"'{args.feature}' does not exist.")

    to_module = impl(args.base)
    if not to_module.is_file():
        raise Exception(f"'{args.base}' does not exist.")

    print("Calculating change...", file=stderr)
    changes = patch(args.feature)
    if not changes:
        print("No changes.", file=stderr)
        return

    print("Porting feature to base...", file=stderr)
    adapted = change_patch_directories(changes, args.feature, args.base)

    if args.dry_run:
        print(adapted)
        return

    if not args.force and not is_repo_clean():
        raise Exception(
            "Working directory or index is not clean, not merging.  Re-run "
            "with --force to ignore warning."
        )

    print("Applying changes...", file=stderr)
    subprocess.run(
        ["git", "apply", "-"],
        input=adapted,
        text=True,
        stdout=stdout,
        stderr=stderr,
    )


def do_patch(args):
    print(patch(args.feature))


def do_rebase(args):
    impl_module = impl(args.feature)
    if not impl_module.is_file():
        raise Exception(f"'{args.feature}' does not exist.")

    if not args.force and not is_repo_clean():
        raise Exception(
            "Working directory or index is not clean, not rebasing.  Re-run "
            "with --force to ignore warning."
        )

    # Need to do this before we delete the existing crates, because it
    # will leave the workspace in an inconsistent state, so we can't
    # build the `cut` binary at that point.
    ensure_cut_binary()

    print("Preserving changes...", file=stderr)
    changes = patch(args.feature) or ""

    print("Cleaning feature...", file=stderr)
    delete_cut_crates(args.feature)

    print("Re-generating feature...", file=stderr)
    cmd = cut_command(args.feature)
    cmd.append("--no-workspace-update")

    result = subprocess.run(cmd, stdout=stdout, stderr=stderr)
    if result.returncode != 0:
        print("Re-generation failed.", file=stderr)
        exit(result.returncode)

    print("Re-applying changes...", file=stderr)
    subprocess.run(
        ["git", "am", "-3", "-"],
        input=changes,
        text=True,
        stdout=stdout,
        stderr=stderr,
    )


def run(command):
    """Run command, and return its stdout as a UTF-8 string."""
    result = subprocess.run(command, stdout=subprocess.PIPE)
    return result.stdout.decode("utf-8")


def repo_root():
    """Find the repository root, using git."""
    return run(["git", "rev-parse", "--show-toplevel"]).strip()


def origin_commit(feature):
    """Find the commit that introduced cut with name `feature`.

    Returns the commit hash as a string if one can be found.  Returns
    `None` if the cut exists but hasn't been committed, and raises an
    `Exception` if the cut does not exist.
    """
    impl_module = impl(feature)
    if not impl_module.is_file():
        raise Exception(f"Cut '{feature}' does not exist.")

    commit = run(
        [
            *["git", "log"],
            "--pretty=format:%H",
            "--diff-filter=A",
            *["--", impl_module],
        ]
    ).strip()

    return None if commit == "" else commit


def is_repo_clean():
    """Checks whether the repo is in a clean state."""
    return run(["git", "status", "--porcelain"]).strip() == ""


def patch(feature):
    sha = origin_commit(args.feature)
    if sha is None:
        return

    return run(
        [
            *["git", "diff", f"{sha}...HEAD", "--"],
            *cut_directories(args.feature),
        ]
    )


def change_patch_directories(patch, feature, base):
    """Fix-up patch referring to `feature` to refer to `base`.

    Look for references to directories in cut `feature` in the
    metadata of patch and replace them with references to the
    equivalent directories in `base`.
    """

    def is_metadata(line):
        return (
            line.startswith("+++")
            or line.startswith("---")
            or line.startswith("diff --git")
        )

    # Mapping from directories that might appear in the patch file to
    # the directories they should be replaced by.  Represented as a
    # list of pairs as it will be iterated over.
    mapping = list(
        zip(
            map(str, cut_directories(feature)),
            map(str, cut_directories(base)),
        )
    )

    def sub(line):
        for feat, base in mapping:
            line = line.replace(feat, base)
        return line

    return "".join(
        (sub(line) if is_metadata(line) else line) + "\n"
        for line in patch.splitlines()
        if not line.startswith("index")
    )


def ensure_cut_binary():
    """Ensure a build of the cut binary exists."""
    result = subprocess.run(
        ["cargo", "build", "--bin", "cut"],
        stdout=stdout,
        stderr=stderr,
    )
    if result.returncode != 0:
        return Exception("Failed to build 'cut' binary.")


def cut_command(f):
    """Arguments for creating the cut for 'feature'."""
    return [
        *["./target/debug/cut", "--feature", f],
        *["-d", f"sui-execution/latest:sui-execution/{f}:-latest"],
        *["-d", f"external-crates/move:external-crates/move/move-execution/{f}"],
        *["-p", "sui-adapter-latest"],
        *["-p", "sui-move-natives-latest"],
        *["-p", "sui-verifier-latest"],
        *["-p", "move-abstract-interpreter"],
        *["-p", "move-bytecode-verifier"],
        *["-p", "move-stdlib-natives"],
        *["-p", "move-vm-runtime"],
        *["-p", "bytecode-verifier-tests"],
    ]


def cut_directories(f):
    """Directories containing crates for `feature`."""
    sui_base = Path() / "sui-execution"
    external = Path() / "external-crates"

    crates = [
        sui_base / f / "sui-adapter",
        sui_base / f / "sui-move-natives",
        sui_base / f / "sui-verifier",
    ]

    if f == "latest":
        crates.extend(
            [
                external / "move" / "crates" / "move-abstract-interpreter",
                external / "move" / "crates" / "move-bytecode-verifier",
                external / "move" / "crates" / "move-stdlib-natives",
                external / "move" / "crates" / "move-vm-runtime",
                external / "move" / "crates" / "bytecode-verifier-tests",
            ]
        )
    else:
        crates.extend(
            [
                external / "move" / "move-execution" / f / "crates" / "move-abstract-interpreter",
                external / "move" / "move-execution" / f / "crates" / "move-bytecode-verifier",
                external / "move" / "move-execution" / f / "crates" / "move-stdlib-natives",
                external / "move" / "move-execution" / f / "crates" / "move-vm-runtime",
                external / "move" / "move-execution" / f / "crates" / "bytecode-verifier-tests",
            ]
        )

    return crates


def impl(feature):
    """Path to the impl module for this feature"""
    return Path() / "sui-execution" / "src" / (feature.replace("-", "_") + ".rs")


def delete_cut_crates(feature):
    """Delete `feature`-specific crates."""
    if feature == "latest":
        raise Exception("Can't delete 'latest'")
    for module in cut_directories(feature):
        rmtree(module)


def update_toml(feature, toml_path):
    """Add dependencies for 'feature' to sui-execution's manifest."""

    # Read all the lines
    with open(toml_path) as toml:
        lines = toml.readlines()

    # Write them back, looking for template comment lines
    with open(toml_path, mode="w") as toml:
        for line in lines:
            if line.startswith("# ") and "$CUT" in line:
                toml.write(line[2:].replace("$CUT", feature))
            toml.write(line)


def generate_impls(feature, copy):
    """Create the implementations of the `Executor` and `Verifier`.

    Copies the implementation for the `latest` cut and updates its imports."""
    orig = Path() / "sui-execution" / "src" / "latest.rs"
    with open(orig, mode="r") as orig, open(copy, mode="w") as copy:
        for line in orig:
            line = re.sub(r"^use (.*)_latest::", rf"use \1_{feature.replace('-', '_')}::", line)
            copy.write(line)


def generate_lib(output_file: TextIO):
    """Expose all `Executor` and `Verifier` impls via lib.rs

    Generates the contents of sui-execution/src/lib.rs to assign a numeric
    execution version for every module that implements an execution version.

    Version snapshots (whose names follow the pattern `/v[0-9]+/`) are assigned
    versions according to their names (v0 gets 0, v1 gets 1, etc).

    `latest` gets the next version after all version snapshots.

    Feature snapshots (all other snapshots) are assigned versions starting with
    `u64::MAX` and going down, in the order they were created (as measured by
    git commit timestamps)

    The generated contents are written out to `output_file` (an IO device).
    """

    template_path = Path() / "sui-execution" / "src" / "lib.template.rs"
    cuts = discover_cuts()

    with open(template_path, mode="r") as template_file:
        template = template_file.read()

    def substitute(m):
        spc = m.group(1)
        var = m.group(2)

        if var == "GENERATED_MESSAGE":
            cmd = "./scripts/execution-layer"
            return f"{spc}// DO NOT MODIFY, Generated by {cmd}"
        elif var == "MOD_CUTS":
            return "".join(sorted(f"{spc}mod {cut};" for (_, _, cut) in cuts))
        elif var == "FEATURE_CONSTS":
            return "".join(
                f"{spc}pub const {feature}: u64 = {version};"
                for (version, feature, _) in cuts
                if feature is not None
            )
        elif var == "EXECUTOR_CUTS":
            executor = (
                "{spc}{version} => Arc::new({cut}::Executor::new(\n"
                "{spc}    protocol_config,\n"
                "{spc}    silent,\n"
                "{spc}    enable_profiler,\n"
                "{spc})?),\n"
            )
            return "\n".join(
                executor.format(spc=spc, version=feature or version, cut=cut)
                for (version, feature, cut) in cuts
            )
        elif var == "VERIFIER_CUTS":
            call = "Verifier::new(config, metrics)"
            return "\n".join(
                f"{spc}{feature or version} => Box::new({cut}::{call}),"
                for (version, feature, cut) in cuts
            )
        else:
            raise Exception(f"Don't know how to substitute {var}")


    rust_code = re.sub(
            r"^(\s*)// \$([A-Z_]+)$",
            substitute,
            template,
            flags=re.MULTILINE,
        )

    try:
        result = subprocess.run(['rustfmt'], input=rust_code, text=True, capture_output=True, check=True)
        formatted_code = result.stdout
        output_file.write(formatted_code)
    except subprocess.CalledProcessError as e:
        print(f"rustfmt failed with error code {e.returncode}")
        print("stderr:", e.stderr)
    except Exception as e:
        print(f"An error occurred: {e}")

# Modules in `sui-execution` that don't count as "cuts" (they are
# other supporting modules)
NOT_A_CUT = {
    "executor",
    "lib",
    "lib.template",
    "tests",
    "verifier",
}


def discover_cuts():
    """Find all modules corresponding to execution layer cuts.

    Finds all modules within the `sui-execution` crate that count as
    entry points for an execution layer cut.  Returns a list of
    3-tuples, where:

    - The 0th element is a string representing the version number.
    - The 1st element is an (optional) constant name for the version,
      used to easily export the versions for features.
    - The 2nd element is the name of the module.

    Snapshot cuts (with names following the pattern /latest|v[0-9]+/)
    are assigned version numbers according to their name (with latest
    getting the version one higher than the highest occupied snapshot
    version).

    Feature cuts (all other cuts) are assigned versions starting with
    `u64::MAX` and counting down, ordering features first by commit
    time, and then by name.
    """

    snapshots = []
    features = []

    src = Path() / "sui-execution" / "src"
    for f in src.iterdir():
        if not f.is_file() or f.stem in NOT_A_CUT:
            continue
        elif re.match(r"latest|v[0-9]+", f.stem):
            snapshots.append(f)
        else:
            features.append(f)

    def snapshot_key(path):
        if path.stem == "latest":
            return float("inf")
        else:
            return int(path.stem[1:])

    def feature_key(path):
        return path.stem

    snapshots.sort(key=snapshot_key)
    features.sort(key=feature_key)

    cuts = []
    for snapshot in snapshots:
        mod = snapshot.stem
        if mod != "latest":
            cuts.append((mod[1:], None, mod))
            continue

        # Latest gets one higher version than any other snapshot
        # version we've assigned so far
        ver = 1 + max(int(v) for (v, _, _) in cuts)
        cuts.append((str(ver), None, "latest"))

    # "Feature" cuts are not intended to be used on production
    # networks, so stability is not as important for them, they are
    # assigned versions in lexicographical order.
    for i, feature in enumerate(features):
        version = f"u64::MAX - {i}" if i > 0 else "u64::MAX"
        feature_stem = feature.stem.replace("-", "_")
        cuts.append((version, feature_stem.upper(), feature_stem))

    return cuts


if __name__ == "__main__":
    for bin in ["git", "cargo"]:
        if not which(bin):
            print(f"Please install '{bin}'", file=stderr)

    args = parse_args()
    chdir(repo_root())

    try:
        args.do(args)
    except Exception as e:
        print(f"{args.cmd} failed!  {e}", file=stderr)
