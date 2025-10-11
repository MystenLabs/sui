#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

"""
Utility to create Swift's compilation DBs

$ make_swift_comp_db.py gen --output=entry.json foo.swift -- -I /path/ -Xcc -fno-implicit-modules
"""

# pyre-unsafe

import argparse
import json
import shlex
import sys


def gen(args):

    with open(args.project_root_file, "r") as project_root_file:
        project_root = project_root_file.read().replace("\n", "")

    entry = {}
    entry["files"] = list(args.files)
    entry["directory"] = project_root

    arguments = []
    for arg in args.arguments:
        if arg.startswith("@"):
            with open(arg[1:]) as argsfile:
                for line in argsfile:
                    # The argsfile's arguments are separated by newlines; we
                    # don't want those included in the argument list.
                    arguments.append(" ".join(shlex.split(line)))
        else:
            arguments.append(arg)
    entry["arguments"] = arguments

    json.dump([entry], args.output, indent=2)
    args.output.close()


def main(argv):
    parser = argparse.ArgumentParser()

    parser.add_argument("--output", type=argparse.FileType("w"), default=sys.stdout)
    parser.add_argument("--files", nargs="*")
    # A path to a file that contains project root
    parser.add_argument("--project-root-file")
    parser.add_argument("arguments", nargs="*")

    args = parser.parse_args(argv[1:])

    gen(args)


sys.exit(main(sys.argv))
