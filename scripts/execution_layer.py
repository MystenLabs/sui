#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
from os import chdir, remove
from shutil import which, rmtree
import subprocess
from sys import stderr, stdout


def parse_args():
    parser = argparse.ArgumentParser(
        prog="execution-layer",
    )

    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the operations, without running them",
    )

    subparsers = parser.add_subparsers(
        description="Tools for managing cuts of the execution-layer",
    )

    cut = subparsers.add_parser(
        "cut",
        help=(
            "Create a new copy of execution-related crates, and add them to "
            "the workspace.  Assigning an execution layer version to the new "
            "copy and implementing the Execution and Verifier traits in "
            "crates/sui-execution must be done manually as a follow-up."
        ),
    )

    cut.set_defaults(do=do_cut)
    cut.add_argument("feature", help="The name of the new cut to make")

    return parser.parse_args()


def do_cut(args):
    """Perform the actions of the 'cut' sub-command.
    Accepts the parsed command-line arguments as a parameter."""
    cmd = cut_command(args.feature)

    if args.dry_run:
        cmd.append("--dry-run")
        print(run(cmd))
    else:
        print("Cutting new release", file=stderr)
        result = subprocess.run(cmd, stdout=stdout, stderr=stderr)

        if result.returncode != 0:
            print("Cut failed", file=stderr)
            exit(result.returncode)

        clean_up_cut(args.feature)
        run(["cargo", "hakari", "generate"])


def run(command):
    """Run command, and return its stdout as a UTF-8 string."""
    return subprocess.run(command, stdout=subprocess.PIPE).stdout.decode("utf-8")


def repo_root():
    """Find the repository root, using git."""
    return run(["git", "rev-parse", "--show-toplevel"]).strip()


def cut_command(feature):
    """Arguments for creating the cut for 'feature'."""
    return [
        *["cargo", "run", "--bin", "cut", "--"],
        *["--feature", feature],
        *["-d", f"sui-execution/latest:sui-execution/{feature}:-latest"],
        *["-d", f"external-crates/move:external-crates/move-execution/{feature}"],
        *["-p", "sui-adapter-latest"],
        *["-p", "sui-move-natives-latest"],
        *["-p", "sui-verifier-latest"],
        *["-p", "move-bytecode-verifier"],
        *["-p", "move-stdlib"],
        *["-p", "move-vm-runtime"],
    ]


def clean_up_cut(feature):
    """Remove some special-case files/directories from a given cut"""
    move_exec = f"external-crates/move-execution/{feature}"
    rmtree(move_exec + "/move-bytecode-verifier/transactional-tests")
    remove(move_exec + "/move-stdlib/src/main.rs")
    rmtree(move_exec + "/move-stdlib/tests")


if __name__ == "__main__":
    for bin in ["git", "cargo", "cargo-hakari"]:
        if not which(bin):
            print(f"Please install '{bin}'", file=stderr)

    args = parse_args()
    chdir(repo_root())
    args.do(args)
