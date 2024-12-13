# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
import os
import re

ROOT = os.path.join(os.path.dirname(__file__), "../")
PATTERN = None

def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", default="move")
    subparser = parser.add_subparsers(
        dest="command",
        description="""
    Automatically manage the dependency path to Move repository.
    Command "local" switches the dependency from git to local path.
    Command "upgrade" upgrades the git revision. A repository can be
    specified if we want to use a fork instead of upstream.
    A revision or a branch also needs to be specified.
    """,
    )
    subparser.add_parser("local")
    upgrade = subparser.add_parser("upgrade")
    upgrade.add_argument("--repo", required=False)
    upgrade_group = upgrade.add_mutually_exclusive_group(required=True)
    upgrade_group.add_argument("--rev")
    upgrade_group.add_argument("--branch")
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


def try_match_line(line):
    # Remove all spacing for easier pattern matching
    line = line.strip().replace(' ', '')
    m = PATTERN.match(line)
    if m:
        name = m.group(1)
        if m.group(2) is None:
            extra = ""
        else:
            # Add some spacing so it looks nicer
            extra = m.group(2).replace(',', ', ').replace('=', ' = ')
        return (name, extra)
    return None

def switch_to_local(project):
    default_path_map = {
        "move": "move/language",
    }
    # Packages that don't directly map to a directory under move/language
    # go here as special cases. By default, we just use language/[name].
    subpath_path_map = {
        "move": {
            "bytecode-interpreter-crypto": "move-prover/interpreter/crypto",
            "move-abigen": "move-prover/move-abigen",
            "move-bytecode-utils": "tools/move-bytecode-utils",
            "move-bytecode-source-map": "move-ir-compiler/move-bytecode-source-map",
            "move-bytecode-viewer": "tools/move-bytecode-viewer",
            "move-docgen": "move-prover/move-docgen",
            "move-prover-boogie-backend": "move-prover/boogie-backend",
            "move-disassembler": "tools/move-disassembler",
            "move-ir-to-bytecode": "move-ir-compiler/move-ir-to-bytecode",
            "move-ir-to-bytecode-syntax": "move-ir-compiler/move-ir-to-bytecode/syntax",
            "move-ir-types": "move-ir/types",
            "move-cli": "tools/move-cli",
            "move-core-types": "move-core/types",
            "move-coverage": "tools/move-coverage",
            "move-package": "tools/move-package",
            "move-resource-viewer": "tools/move-resource-viewer",
            "move-stackless-bytecode": "move-prover/bytecode",
            "move-unit-test": "tools/move-unit-test",
            "move-vm-test-utils": "move-vm/test-utils",
            "move-vm-runtime": "move-vm/runtime",
            "move-vm-types": "move-vm/types",
            "move-table-extension": "extensions/move-table-extension",
            "move-transactional-test-runner": "testing-infra/transactional-test-runner",
        },
    }

    def process_line(line, depth):
        m = try_match_line(line)
        if m:
            (name, extra) = m
            go_back = "".join(["../"] * (depth + 1))
            path = default_path_map[project]
            if project == "move":
                subpath = subpath_path_map[project].get(name, name)
            return '{} = {{ path = "{}{}/{}"{} }}\n'.format(
                name, go_back, path, subpath, extra
            )
        return line

    scan_files(ROOT, process_line)


def upgrade_revision(project, repo, rev, branch):
    assert (args.rev is None) != (args.branch is None)
    def process_line(line, _):
        m = try_match_line(line)
        if m:
            (name, extra) = m
            return '{} = {{ git = "https://github.com/{}/{}", {} = "{}"{} }}\n'.format(
                name, repo, project,
                "branch" if branch else "rev",
                branch if branch else rev,
                extra
            )
        return line

    scan_files(ROOT, process_line)


args = parse_args()
assert(args.project == "move")

PATTERN = re.compile(
    '(.+)={git="https://github.com/.+/' + args.project + '",(?:rev|branch)="[^"]+"(,.*)?}'
)

if args.command == "local":
    switch_to_local(args.project)
else:
    assert args.command == "upgrade"
    repo = args.repo
    if not repo:
        if args.project == "move":
            repo = "move-language"
        else:
            repo = "MystenLabs"
    upgrade_revision(args.project, repo, args.rev, args.branch)
