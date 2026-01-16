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
    "consensus",
    "crates",
    "dashboards",
    "doc",
    "docker",
    "external-crates",
    "kiosk",
    "nre",
    "sui-execution",
]

# Start release notes with these sections, if they contain relevant
# information (helps us keep a consistent order for impact areas we
# know about).
NOTE_ORDER = [
    "Protocol",
    "Nodes (Validators and Full nodes)",
    "gRPC",
    "JSON-RPC",
    "GraphQL",
    "CLI",
    "Rust SDK",
    "Indexing Framework",
]


class Note(NamedTuple):
    checked: bool
    note: str


def parse_args():
    """Parse command line arguments."""

    parser = argparse.ArgumentParser(
        description=(
            "Extract release notes from git commits. Check help for the "
            "`generate`, `check`, and `list-prs` subcommands for more information."
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
            "Check if the release notes section of a the PR is complete, "
            "i.e. that every impacted component has a non-empty note."
        ),
    )

    check_p.add_argument(
        "pr",
        nargs="?",
        help="The PR to check.",
    )

    list_prs_p = sub_parser.add_parser(
        "list-prs",
        description=(
            "List PRs with release notes between two commits. "
            "Outputs JSON with PR numbers and metadata for CI workflows."
        ),
    )

    list_prs_p.add_argument(
        "from",
        help="The commit to start from (exclusive)",
    )

    list_prs_p.add_argument(
        "to",
        nargs="?",
        default="HEAD",
        help="The commit to end at (inclusive), defaults to HEAD.",
    )

    get_notes_p = sub_parser.add_parser(
        "get-notes",
        description="Get release notes for a specific PR, formatted for display.",
    )

    get_notes_p.add_argument(
        "pr",
        help="The PR number to get release notes for.",
    )

    return vars(parser.parse_args())


def git(*args):
    """Run a git command and return the output as a string."""
    return subprocess.check_output(["git"] + list(args)).decode().strip()


def gh_api(endpoint):
    """Call GitHub API using gh CLI and return JSON response."""
    result = subprocess.run(
        ["gh", "api", "-H", "Accept: application/vnd.github+json", endpoint],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)


def parse_notes(pr, notes):
    # # Find the release notes section
    result = {}
    match = RE_HEADING.search(notes)
    if not match:
        return pr, result

    start = 0
    notes = match.group(1)

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


def extract_notes_for_pr(pr):
    """Get release notes from a body of the PR

    Find the 'Release notes' section in PR body, and
    extract the notes for each impacted area (area that has been
    ticked).

    Returns a tuple of the PR number and a dictionary of impacted
    areas mapped to their release note. Each release note indicates
    whether it has a note and whether it was checked (ticked).

    """
    data = gh_api(f"/repos/MystenLabs/sui/pulls/{pr}")
    if not data:
        return pr, {}
    notes = data.get("body") or ""
    return parse_notes(pr, notes)


def extract_notes_for_commit(commit):
    """Get release notes from a commit message.

    Find the 'Release notes' section in the commit message, and
    extract the notes for each impacted area (area that has been
    ticked).

    Returns a tuple of the PR number and a dictionary of impacted
    areas mapped to their release note. Each release note indicates
    whether it has a note and whether it was checked (ticked).

    """
    message = git("show", "-s", "--format=%B", commit)

    # Extract PR number
    match = RE_PR.match(message)
    pr = match.group(1) if match else None
    return parse_notes(pr, message)


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


def do_check(pr):
    """Check if the release notes section of a given PR is complete.

    This means that every impacted component has a non-empty note,
    every note is attached to a checked checkbox, and every impact
    area is known.

    """

    pr, notes = extract_notes_for_pr(pr)
    issues = []
    for impacted, note in notes.items():
        if impacted not in NOTE_ORDER:
            issues.append(f" - Found unfamiliar impact area '{impacted}'.")

        if note.checked and not note.note:
            issues.append(
                f" - '{impacted}' is checked but has no release note.")

        if not note.checked and note.note:
            issues.append(
                f" - '{impacted}' has a release note but is not checked: {note.note}"
            )

    if not issues:
        return

    print(f"Found issues with release notes in PR {pr}:")
    for issue in issues:
        print(issue)
    sys.exit(1)


def get_pr_for_commit(commit):
    """Get the PR number associated with a commit using GitHub API."""
    data = gh_api(f"/repos/MystenLabs/sui/commits/{commit}/pulls")
    if data and len(data) > 0:
        return data[0].get("number")
    return None


def pr_has_release_notes(pr):
    """Check if a PR has any checked release notes boxes."""
    data = gh_api(f"/repos/MystenLabs/sui/pulls/{pr}")
    if not data:
        return False
    body = data.get("body", "") or ""
    return bool(re.search(r"^\s*-\s*\[x\]\s*", body, re.MULTILINE | re.IGNORECASE))


def do_list_prs(from_, to):
    """List PRs with release notes between two commits.

    Outputs JSON with PR numbers that have checked release notes,
    suitable for use in CI workflows.
    """
    root = git("rev-parse", "--show-toplevel")
    os.chdir(root)

    commits = git(
        "log",
        "--pretty=format:%H",
        f"{from_}..{to}",
        "--",
        *INTERESTING_DIRECTORIES,
    ).strip()

    if not commits:
        print("[]")
        return

    seen_prs = set()
    prs_with_notes = []

    for commit in commits.split("\n"):
        pr = get_pr_for_commit(commit)
        if pr and pr not in seen_prs:
            seen_prs.add(pr)
            if pr_has_release_notes(pr):
                prs_with_notes.append(pr)

    print(json.dumps(prs_with_notes))


def do_get_notes(pr):
    """Get formatted release notes for a specific PR."""
    _, notes = extract_notes_for_pr(pr)

    output_lines = []
    for impacted, note in notes.items():
        if note.checked and note.note:
            output_lines.append(f"{impacted}: {note.note}")

    print("\n".join(output_lines))


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

    for commit in commits.split("\n"):
        pr, notes = extract_notes_for_commit(commit)
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
            print(f"#### Sui Protocol Version in this release: `{protocol_version}`")
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


if __name__ == "__main__":
    args = parse_args()
    if args["command"] is None:
        print("Error: No command provided.\n", file=sys.stderr)
        subprocess.run([sys.executable, __file__, "--help"])
        sys.exit(1)
    elif args["command"] == "generate":
        do_generate(args["from"], args["to"])
    elif args["command"] == "check":
        do_check(args["pr"])
    elif args["command"] == "list-prs":
        do_list_prs(args["from"], args["to"])
    elif args["command"] == "get-notes":
        do_get_notes(args["pr"])
