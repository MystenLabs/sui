# Copyright (c) Facebook, Inc. and its affiliates.
# SPDX-License-Identifier: Apache-2.0

import argparse
import os
import re

CURRENT_REV = "661a2d1367a64a02027e4ed8f4b18f0a37cfaa17"
ROOT = os.path.join(os.path.dirname(__file__), "../")
PATTERN = re.compile(
    '(\s*)(.+) = {{ git = "https://github.com/diem/diem", rev="{}" }}(\s*)'.format(
        CURRENT_REV
    )
)


def parse_args():
    parser = argparse.ArgumentParser()
    subparser = parser.add_subparsers(
        dest="command",
        description="""
    Automatically manage the dependency path to Diem repository.
    Command "local" switches the dependency from git to local path.
    Command "upgrade" upgrades the git revision.
    """,
    )
    local = subparser.add_parser("local")
    remote = subparser.add_parser("remote")
    upgrade = subparser.add_parser("upgrade")
    upgrade.add_argument("--rev", type=str, required=True)
    return parser.parse_args()


def scan_file(file, process_line, depth=0):
    new_content = []
    with open(file) as f:
        for line in f.readlines():
            new_content.append(process_line(line, depth))
    with open(file, "w") as f:
        f.writelines(new_content)


def scan_files(path, process_line, depth=0):
    for file in os.listdir(path):
        full_path = os.path.join(path, file)
        if os.path.isdir(full_path):
            scan_files(full_path, process_line, depth + 1)
        elif file == "Cargo.toml":
            scan_file(full_path, process_line, depth)


def switch_to_local():
    # Packages that don't directly map to a directory under diem/language
    # go here as special cases. By default, we just use language/[name].
    path_map = {
        "move-bytecode-utils": "tools/move-bytecode-utils",
        "move-cli": "tools/move-cli",
        "move-core-types": "move-core/types",
        "move-package": "tools/move-package",
        "move-vm-runtime": "move-vm/runtime",
        "move-vm-types": "move-vm/types",
    }

    def process_line(line, depth):
        m = PATTERN.match(line)
        if m:
            prefix = m.group(1)
            name = m.group(2)
            postfix = m.group(3)
            go_back = "".join(["../"] * (depth + 1))
            return '{}{} = {{ path = "{}diem/language/{}" }}{}'.format(
                prefix, name, go_back, path_map.get(name, name), postfix
            )
        return line

    scan_files(ROOT, process_line)


def upgrade_revision(rev):
    def process_line(line, _):
        return line.replace(CURRENT_REV, rev)

    scan_files(ROOT, process_line)
    # Also patch the script itself with the new revision.
    scan_file(__file__, process_line)


args = parse_args()
if args.command == "local":
    switch_to_local()
else:
    assert args.command == "upgrade"
    upgrade_revision(args.rev)