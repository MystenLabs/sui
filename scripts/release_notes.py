#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
from collections import defaultdict
import json
import os
import re
import subprocess
import sys
from typing import NamedTuple
import urllib.request

RE_NUM = re.compile("[0-9_]+")

RE_PR = re.compile(
    r"^.*\(#(\d+)\)$",
    re.MULTILINE,
)

RE_HEADING = re.compile(
    r"#+ Release notes(.*)",
    re.DOTALL | re.IGNORECASE,
)

RE_CHECK = re.compile(
    r"^\s*-\s*\[.\]",
    re.MULTILINE,
)

RE_NOTE = re.compile(
    r"^\s*-\s*\[( |x)?\]\s*([^:]+):",
    re.MULTILINE | re.IGNORECASE,
)

# Only commits that affect changes in these directories will be
# considered when generating release notes.
INTERESTING_DIRECTORIES = [
    "crates",
    "dashboards",
    "doc",
    "docker",
    "external-crates",
    "kiosk",
    "narwhal",
    "nre",
    "sui-execution",
]

# Start release notes with these sections, if they contain relevant
# information (helps us keep a consistent order for impact areas we
# know about).
NOTE_ORDER = [
    "Protocol",
    "Nodes (Validators and Full nodes)",
    "Indexer",
    "JSON-RPC",
    "GraphQL",
    "CLI",
    "Rust SDK",
    "REST API",
]


class Note(NamedTuple):
    checked: bool
    note: str


def parse_args():
    """Parse command line arguments."""

    parser = argparse.ArgumentParser(
        description=(
            "Extract release notes from git commits. Check help for the "
            "`generate` and `check` subcommands for more information."
        ),
    )

    sub_parser = parser.add_subparsers(dest="command")

    generate_p = sub_parser.add_parser(
        "generate",
        description="Generate release notes from git commits.",
    )

    generate_p.add_argument(
        "from",
        help="The commit to start from (exclusive)",
    )

    generate_p.add_argument(
        "to",
        nargs="?",
        default="HEAD",
        help="The commit to end at (inclusive), defaults to HEAD.",
    )

    check_p = sub_parser.add_parser(
        "check",
        description=(
            "Check if the release notes section of a givne commit is complete, "
            "i.e. that every impacted component has a non-empty note."
        ),
    )

    check_p.add_argument(
        "commit",
        nargs="?",
        default="HEAD",
        help="The commit to check, defaults to HEAD.",
    )

    return vars(parser.parse_args())


def git(*args):
    """Run a git command and return the output as a string."""
    return subprocess.check_output(["git"] + list(args)).decode().strip()


def extract_notes_from_rebase_commit(commit):
    # we'll need to go one level deeper to find the PR number
    url = f"https://api.github.com/repos/MystenLabs/sui/commits/{commit}/pulls"
    headers = {
        "Accept": "application/vnd.github.v3+json",
    }
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as response:
        data = json.load(response)
        if len(data) == 0:
            return None, ""
        pr_number = data[0]["number"]
        pr_notes = data[0]["body"] if data[0]["body"] else ""
        return pr_number, pr_notes


def extract_notes(commit, seen):
    """Get release notes from a commit message.

    Find the 'Release notes' section in the commit message, and
    extract the notes for each impacted area (area that has been
    ticked).

    Returns a tuple of the PR number and a dictionary of impacted
    areas mapped to their release note. Each release note indicates
    whether it has a note and whether it was checked (ticked).

    """
    message = git("show", "-s", "--format=%B", commit)

    # Extract PR number from squashed commits
    match = RE_PR.match(message)
    pr = match.group(1) if match else None

    result = {}

    notes = ""
    if pr is None:
        # Extract PR number from rebase commits if it's not a squashed commit
        pr, notes = extract_notes_from_rebase_commit(commit)
    else:
        # Otherwise, find the release notes section from the squashed commit message
        match = RE_HEADING.search(message)
        if not match:
            return pr, result
        notes = match.group(1)

    if pr in seen:
        # a PR can be in multiple commits if it's from a rebase,
        # so we only want to process it once
        return pr, result

    start = 0
    while True:
        # Find the next possible release note
        match = RE_NOTE.search(notes, start)
        if not match:
            break

        checked = match.group(1)
        impacted = match.group(2)
        begin = match.end()

        # Find the end of the note, or the end of the commit
        match = RE_CHECK.search(notes, begin)
        end = match.start() if match else len(notes)

        result[impacted] = Note(
            checked=checked in "xX",
            note=notes[begin:end].strip(),
        )
        start = end

    return pr, result


def extract_protocol_version(commit):
    """Find the max protocol version at this commit.

    Assumes that it is being called from the root of the sui repository."""
    for line in git(
        "show", f"{commit}:crates/sui-protocol-config/src/lib.rs"
    ).splitlines():
        if "const MAX_PROTOCOL_VERSION" not in line:
            continue

        _, _, assign = line.partition("=")
        if not assign:
            continue

        match = RE_NUM.search(assign)
        if not match:
            continue

        return match[0]


def print_changelog(pr, log):
    if pr:
        print(f"https://github.com/MystenLabs/sui/pull/{pr}:")
    print(log)


def do_check(commit):
    """Check if the release notes section of a given commit is complete.

    This means that every impacted component has a non-empty note,
    every note is attached to a checked checkbox, and every impact
    area is known.

    """

    _, notes = extract_notes(commit, set())
    issues = []
    for impacted, note in notes.items():
        if impacted not in NOTE_ORDER:
            issues.append(f" - Found unfamiliar impact area '{impacted}'.")

        if note.checked and not note.note:
            issues.append(f" - '{impacted}' is checked but has no release note.")

        if not note.checked and note.note:
            issues.append(
                f" - '{impacted}' has a release note but is not checked: {note.note}"
            )

    if not issues:
        return

    print(f"Found issues with release notes in {commit}:")
    for issue in issues:
        print(issue)
    sys.exit(1)


def do_generate(from_, to):
    """Generate release notes from git commits.

    This will extract the release notes from all commits between
    `from_` (exclusive) and `to` (inclusive), and print out a markdown
    snippet with a heading for each impact area that has a note,
    followed by a list of its relevant changelog.

    Only looks for commits affecting INTERESTING_DIRECTORIES.

    Additionally injects the current protocol version into the
    "Protocol" changelog.

    """
    results = defaultdict(list)

    root = git("rev-parse", "--show-toplevel")
    os.chdir(root)

    protocol_version = extract_protocol_version(to) or "XX"

    commits = git(
        "log",
        "--pretty=format:%H",
        f"{from_}..{to}",
        "--",
        *INTERESTING_DIRECTORIES,
    ).strip()

    if not commits:
        return

    seen_prs = set()
    for commit in commits.split("\n"):
        pr, notes = extract_notes(commit, seen_prs)
        seen_prs.add(pr)
        for impacted, note in notes.items():
            if note.checked:
                results[impacted].append((pr, note.note))

    # Print the impact areas we know about first
    for impacted in NOTE_ORDER:
        notes = results.pop(impacted, None)
        if not notes:
            continue

        print(f"## {impacted}")

        if impacted == "Protocol":
            print(f"Sui Protocol Version in this release: {protocol_version}")
        print()

        for pr, note in reversed(notes):
            print_changelog(pr, note)
            print()

    # Print any remaining impact areas
    for impacted, notes in results.items():
        print(f"## {impacted}\n")
        for pr, note in reversed(notes):
            print_changelog(pr, note)
            print()


args = parse_args()
if args["command"] == "generate":
    do_generate(args["from"], args["to"])
elif args["command"] == "check":
    do_check(args["commit"])
