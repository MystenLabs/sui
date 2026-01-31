#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import argparse
from collections import defaultdict
import json
import os
import re
import shutil
import subprocess
import sys
from typing import NamedTuple

RE_NUM = re.compile("[0-9_]+")

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

# Path to the protocol config file that contains MAX_PROTOCOL_VERSION.
PROTOCOL_CONFIG_PATH = "crates/sui-protocol-config/src/lib.rs"

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


# GraphQL query to fetch the body of a PR by its number.
PR_BODY_QUERY = """
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      body
    }
  }
}
"""

# GraphQL query to fetch `BATCH_SIZE` the associated PRs for a list of commits, by their SHAs.
BATCH_SIZE = 50
COMMIT_QUERY = """
query($owner: String!, $name: String!, {params}) {{
  repository(owner: $owner, name: $name) {{
      {bodies}
  }}
}}
""".format(
    params=",".join(f"$sha{i}: GitObjectID" for i in range(BATCH_SIZE)),
    bodies="\n".join(
        f"""
        commit{i}: object(oid: $sha{i}) {{
          ... on Commit {{
            associatedPullRequests(first: 1) {{
              nodes {{
                number
                body
              }}
            }}
          }}
        }}
        """
        for i in range(BATCH_SIZE)
    ),
)

# Set up a way to make requests to GitHub's GraphQL API
GH_TOKEN = os.getenv("GH_TOKEN")
GH_CLI_PATH = shutil.which("gh")
if GH_TOKEN:
    gql = lambda query, variables: gh_api_request(GH_TOKEN, query, variables)
elif GH_CLI_PATH:
    gql = lambda query, variables: gh_cli_request(GH_CLI_PATH, query, variables)
else:
    print("Error: GH_TOKEN not set and `gh` CLI not found.", file=sys.stderr)
    sys.exit(1)


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


def parse_notes(notes):
    """Find the release notes in the PR body"""
    result = {}
    if not notes:
        return result

    match = RE_HEADING.search(notes)
    if not match:
        return result

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

    return result


def gh_api_request(token, query, variables):
    """Make a GitHub GraphQL API request using curl.

    Requires an authorization token to be supplied.
    """
    response = subprocess.run(
        [
            "curl",
            "-s",
            *("-H", "Content-Type: application/json"),
            *("-H", f"Authorization: Bearer {token}"),
            *("-d", json.dumps({"query": query, "variables": variables})),
            "https://api.github.com/graphql",
        ],
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        text=True,
    )

    body = json.loads(response.stdout.strip())
    if "errors" in body:
        print(f"GraphQL Error: {body['errors']}", file=sys.stderr)

    return body


def gh_cli_request(cli, query, variables):
    """Make a GitHub GraphQL API request using the gh CLI.

    Piggybacks off the authorization of the gh CLI to run queries. Assumes the
    `gh` binary is available in PATH.
    """
    command = [cli, "api", "graphql", "-f", f"query={query}"]
    for key, value in variables.items():
        if isinstance(value, bool):
            arg_value = "true" if value else "false"
        elif value is None:
            arg_value = "null"
        else:
            arg_value = str(value)
        command.extend(["-F", f"{key}={arg_value}"])

    response = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        text=True,
    )

    body = json.loads(response.stdout.strip())
    if "errors" in body:
        print(f"GraphQL Error: {body['errors']}", file=sys.stderr)

    return body


def extract_notes_for_pr(pr):
    """Get release notes from a body of the PR

    Find the 'Release notes' section in PR body, and
    extract the notes for each impacted area (area that has been
    ticked).

    Returns a dictionary of impacted areas mapped to their release note.
    Each release note indicates whether it has a note and whether it was
    checked (ticked).

    """

    variables = {"owner": "MystenLabs", "name": "sui", "number": int(pr)}
    data = gql(PR_BODY_QUERY, variables)
    body = data.get("data", {}).get("repository", {}).get("pullRequest", {}).get("body")
    return parse_notes(body)


def fetch_release_notes_for_commits(commits):
    """Translates a list of commit SHAs into their release notes.

    Returns a dictionary mapping impacted areas to a list of tuples PR Number
    and Release Note. PRs are de-duplicated.
    """
    seen = set()
    results = defaultdict(list)
    for start in range(0, len(commits), BATCH_SIZE):
        chunk = commits[start : start + BATCH_SIZE]
        variables = {"owner": "MystenLabs", "name": "sui"}
        for i, sha in enumerate(chunk):
            variables[f"sha{i}"] = sha

        data = gql(COMMIT_QUERY, variables)

        repo = data.get("data", {}).get("repository", {})
        for index, sha in enumerate(chunk):
            key = f"commit{index}"
            nodes = repo.get(key, {}).get("associatedPullRequests", {}).get("nodes", [])

            if not nodes:
                print(f"Warning: no PRs found for commit {sha}.", file=sys.stderr)
                continue

            number = nodes[0].get("number")
            body = nodes[0].get("body")

            if not number:
                print(f"Warning: Missing PR number for commit {sha}.", file=sys.stderr)
                continue

            if number in seen:
                continue

            seen.add(number)

            notes = parse_notes(body)
            for impacted, note in notes.items():
                if note.checked:
                    results[impacted].append((number, note.note))

    return results


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
        print(f"https://github.com/MystenLabs/sui/pull/{pr}: {log}")
    else:
        print(log)


def do_check(pr):
    """Check if the release notes section of a given PR is complete.

    This means that every impacted component has a non-empty note,
    every note is attached to a checked checkbox, and every impact
    area is known.

    Additionally, if the PR bumps MAX_PROTOCOL_VERSION, it must have a
    checked 'Protocol' release note with content.

    """

    notes = extract_notes_for_pr(pr)
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

    # Check if the PR bumps MAX_PROTOCOL_VERSION and requires a Protocol release note
    if pr_bumps_protocol_version(pr):
        protocol_note = notes.get("Protocol")
        if not protocol_note:
            issues.append(" - PR bumps MAX_PROTOCOL_VERSION but has no 'Protocol' release note.")
        elif not protocol_note.checked:
            issues.append(" - PR bumps MAX_PROTOCOL_VERSION but 'Protocol' checkbox is not checked.")
        elif not protocol_note.note:
            issues.append(" - PR bumps MAX_PROTOCOL_VERSION but 'Protocol' release note is empty.")

    if not issues:
        return

    print(f"Found issues with release notes in PR {pr}:")
    for issue in issues:
        print(issue)
    sys.exit(1)


def pr_has_release_notes(pr):
    """Check if a PR has any checked release notes boxes."""
    variables = {"owner": "MystenLabs", "name": "sui", "number": int(pr)}
    data = gql(PR_BODY_QUERY, variables)
    body = data.get("data", {}).get("repository", {}).get("pullRequest", {}).get("body") or ""
    return bool(re.search(r"^\s*-\s*\[x\]\s*", body, re.MULTILINE | re.IGNORECASE))


def pr_bumps_protocol_version(pr):
    """Check if a PR modifies MAX_PROTOCOL_VERSION.

    Uses `gh pr diff` to get the PR diff and checks if the protocol config
    file has changes to the MAX_PROTOCOL_VERSION constant.
    """
    if not GH_CLI_PATH:
        return False

    try:
        diff = subprocess.check_output(
            [GH_CLI_PATH, "pr", "diff", str(pr), "--repo", "MystenLabs/sui"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        return False

    in_protocol_config = False
    for line in diff.splitlines():
        if line.startswith("diff --git"):
            in_protocol_config = PROTOCOL_CONFIG_PATH in line
        elif in_protocol_config and line.startswith(("+", "-")) and not line.startswith(("+++", "---")):
            if "MAX_PROTOCOL_VERSION" in line:
                return True
    return False


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

    commit_list = commits.split("\n")

    # Use batched query to get PR info for all commits efficiently
    seen_prs = set()
    prs_with_notes = []

    for start in range(0, len(commit_list), BATCH_SIZE):
        chunk = commit_list[start : start + BATCH_SIZE]
        variables = {"owner": "MystenLabs", "name": "sui"}
        for i, sha in enumerate(chunk):
            variables[f"sha{i}"] = sha

        data = gql(COMMIT_QUERY, variables)
        repo = data.get("data", {}).get("repository", {})

        for index, sha in enumerate(chunk):
            key = f"commit{index}"
            nodes = repo.get(key, {}).get("associatedPullRequests", {}).get("nodes", [])

            if not nodes:
                continue

            number = nodes[0].get("number")
            body = nodes[0].get("body") or ""

            if not number or number in seen_prs:
                continue

            seen_prs.add(number)

            # Check if PR has checked release notes
            if re.search(r"^\s*-\s*\[x\]\s*", body, re.MULTILINE | re.IGNORECASE):
                prs_with_notes.append(number)

    print(json.dumps(prs_with_notes))


def do_get_notes(pr):
    """Get formatted release notes for a specific PR."""
    notes = extract_notes_for_pr(pr)

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

    results = fetch_release_notes_for_commits(commits.split("\n"))

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
