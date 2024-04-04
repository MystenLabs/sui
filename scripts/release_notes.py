#!/usr/bin/env python3

import subprocess
import json
import os
import re
import sys

if len(sys.argv) < 2:
    #      0         1         2         3         4         5         6         7         8
    #      012345678901234567890123456789012345678901234567890123456789012345678901234567890
    print("Usage: ./release_notes.py <ancestor-sha>", file=sys.stderr)
    print()
    print("Extract release notes from git commits from <ancestor-sha> (exclusive) to HEAD", file=sys.stderr)
    print("(inclusive).", file=sys.stderr)
    sys.exit(1)


def git(*args):
    return subprocess.check_output(["git"] + list(args)).decode()


RE_HEADING = re.compile(
    r"#+ Release notes(.*)",
    re.DOTALL | re.IGNORECASE,
)

RE_CHECK = re.compile(
    r"^\s*-\s*\[.\]",
    re.MULTILINE,
)

RE_NOTE = re.compile(
    r"^\s*-\s*\[x\]\s*([^:]+):",
    re.MULTILINE | re.IGNORECASE,
)

before = sys.argv[1]
results = []

for commit in git("log", "--pretty=format:%H", f"{before}..HEAD").split("\n"):
    message = git("show", "-s", "--format=%B", commit)

    match = RE_HEADING.search(message)
    if not match:
        continue

    start = 0
    notes = match.group(1)
    result = {"sha": commit, "notes": {}}

    while True:
        # Find the next checked release note
        match = RE_NOTE.search(notes, start)
        if not match:
            break

        impacted = match.group(1)
        begin = match.end()

        # Find the end of the note, or the end of the commit
        match = RE_CHECK.search(notes, begin)
        end = match.start() if match else len(notes)

        result["notes"][impacted] = notes[begin:end].strip()
        start = end

    if result["notes"]:
        results.append(result)

json.dump(results, sys.stdout, indent=2)
