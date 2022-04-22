#!/bin/bash
# Prereqs: Pandoc installed and in PATH - https://pandoc.org/
# Usage: convert a directory of files from LaTex to Markdown
# Run `latex-md.sh` from the directory containing the files
# Output appears in the same directory
shopt -s nullglob
for f in *.tex
do
        echo "Converting to Markdown for LaTex file - $f"
        pandoc "$f" -f latex -t markdown -o "$f".md
done
# unset it now
shopt -u nullglob
