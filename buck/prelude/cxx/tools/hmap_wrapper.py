#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import itertools
import json
import os
import shlex
import subprocess
import sys
import tempfile


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--hmap-tool", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--mappings-file", required=True)
    parser.add_argument("--project-root-file", required=False)
    args = parser.parse_args(argv[1:])

    with open(args.mappings_file, "r") as argsfile:
        mapping_args = shlex.split(argsfile.read())

    if len(mapping_args) % 2 != 0:
        parser.error("mappings must be dest-source pairs")

    project_root = None
    if args.project_root_file:
        with open(args.project_root_file) as file:
            project_root = file.read().strip()

    # Convert the hmap mappings passed on the command line to a dict.
    mappings = {}
    for src, dst in itertools.zip_longest(*([iter(mapping_args)] * 2)):
        if project_root:
            dst = f"{project_root}/{dst}"
        mappings[src] = dst

        # NOTE(agallagher): Add a mapping from the mapped path to itself. If
        # this is not present, clang will use the mapped path as the new key
        # and continue searching subsequent header maps, which has a couple
        # implications: a) it's slower, as we still search every header map and
        # b) it means we need to use a `-I` anchor to finally terminate the
        # search.
        mappings[dst] = dst

    # Write out the mappings to a JSON file that LLVM's hmaptool accepts.
    with tempfile.TemporaryDirectory() as td:
        output_filename = os.path.join(td, "output")
        with open(output_filename, mode="w") as tf:
            json.dump({"mappings": mappings}, tf, sort_keys=True, indent=2)

        # Delegate to LLVM's hmaptool to generate the hmap.
        subprocess.check_call(
            [sys.executable, args.hmap_tool, "write", output_filename, args.output]
        )


sys.exit(main(sys.argv))
