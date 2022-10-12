#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Prereqs: Pandoc installed and in PATH - https://pandoc.org/
# Usage: convert a directory of files from LaTex to Markdown
# Run `latex-md.sh` from the directory containing the files
# Output appears in the same directory; `mv *.md` as needed
command -v pandoc >/dev/null 2>&1 || { echo "pandoc (https://pandoc.org/) is not installed or missing from PATH, exiting."; return 1; }
shopt -s nullglob
set -e
set -o pipefail
for f in *.tex
do
        echo "Converting to Markdown for LaTex file - $f"
        pandoc "$f" -f latex -t markdown -o "$f".md
done
# unset it now
shopt -u nullglob
