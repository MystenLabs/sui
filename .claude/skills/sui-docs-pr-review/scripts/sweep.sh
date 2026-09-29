#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Mechanical style-guide sweep for Sui docs pages.
# Usage: sweep.sh file.mdx [file.mdx ...]
# Prints the matched marker for every hit so the output is checkable.
# Finds mechanical violations only. It cannot find a wrong claim.

set -uo pipefail
[ $# -eq 0 ] && { echo "usage: $0 <file.mdx>..." >&2; exit 2; }

hdr() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
# grep that reports "clean" instead of silence, so a no-hit check is visible.
g() { local label="$1"; shift; local out; out=$(grep -nE "$@" 2>/dev/null); \
      if [ -n "$out" ]; then printf '%s\n' "$out"; else echo "  (clean) $label"; fi; }

FILES=("$@")

hdr "Banned characters and punctuation"
g "no em/en dashes"      '—|–' "${FILES[@]}"
g "no ampersand in prose" ' & ' "${FILES[@]}"
g "no exclamation (code excluded below)" '!' "${FILES[@]}" | grep -v 'vec!\|assert!\|println!\|panic!\|\.unwrap' || true
g "no straight quotes in prose" '"' "${FILES[@]}" | grep -vE ':[0-9]+:\s*(let|const|use |import|target:|Identifier|vec!|"0x|\s+")' || true

hdr "Banned terms"
g "no Latin abbreviations" '\be\.g\.|\bi\.e\.|\betc\.|\bet al\.' "${FILES[@]}"
g "no 'may'"        '\bmay\b' "${FILES[@]}"
g "no 'via'"        '\bvia\b' "${FILES[@]}"
g "no causal 'since'" '\bsince\b|\bSince\b' "${FILES[@]}"
g "no 'simple/simply'" '\bsimpl[ey]\b' "${FILES[@]}"
g "no 'dApp' outside dApp Kit" 'dApp' "${FILES[@]}"
g "no JSON-RPC"     'json-?rpc' "${FILES[@]}"
g "no leading Note" '^\s*(\*\*)?(Please note|Note)(\*\*)?[:,]' "${FILES[@]}"
g "no 'has to'"     '\bha[sve] to\b' "${FILES[@]}"
g "no 'in order to'" '\bin order to\b' "${FILES[@]}"

hdr "First person (frontmatter questions: are house convention, headings are not)"
g "no first person" '\b(I|I'"'"'m|we|We|our|Our|us)\b' "${FILES[@]}"
hdr "  ^ of those, first person in HEADINGS is the finding:"
g "no first-person headings" '^#{2,4} .*\b(I|my|me)\b' "${FILES[@]}"

hdr "Spelled-out numbers (guide wants numerals for counts)"
# two-ten flagged broadly: legitimate prose uses are rare and worth eyeballing.
# 'one' only before a count noun, since "one of", "one address" are fine.
g "numerals for counts" -i '\b(two|three|four|five|six|seven|eight|nine|ten)\b|\bone (parts|steps|kinds|ways|types|items|objects|things|options|reasons)\b' "${FILES[@]}"

hdr "Idioms and culturally specific phrasing"
g "no idioms" 'up front|in mind|safety net|rules out|cut it off|top up|gets around|out of the box|under the hood|rule of thumb|at the end of the day|keep an eye|hand in hand|a whole class of' "${FILES[@]}"

hdr "Passive-voice candidates (judge each; flag only obvious rewrites)"
# optional adverb slot: catches "is only debited", "are never charged"
g "active voice" '\b(is|are|was|were|be|been|being) +([a-z]+ly|only|never|always|still|also|just) *[a-z]*(ed|en)\b|\b(is|are|was|were|be|been|being) +[a-z]+(ed|en)\b' "${FILES[@]}"

hdr "Formatting"
# Bold table headers are CORRECT and widely used. Never add a check for them.
g "no italics" '(^|[^*])\*[^*`]+\*([^*]|$)' "${FILES[@]}"
g "no manual footers" '^#{2,3} *(Related topics|Related links|Next steps)' "${FILES[@]}"

hdr "Headings (titles Title Case, sections sentence case)"
for f in "${FILES[@]}"; do
  echo "--- $f"
  grep -nE '^title:|^sidebar_label:|^#{1,4} ' "$f" 2>/dev/null
done

hdr "Code fences (tag presence only; READ each block for tag correctness)"
for f in "${FILES[@]}"; do
  n=$(grep -c '^```' "$f" 2>/dev/null || echo 0)
  bare=$(grep -nE '^```$' "$f" 2>/dev/null | wc -l | tr -d ' ')
  echo "--- $f: $n fence lines, $((n/2)) blocks, $bare bare closers"
  grep -nE '^```[a-zA-Z]' "$f" 2>/dev/null | sed 's/^/    /'
done

hdr "Admonitions (max 4 per page; prefer :::tip / :::info over :::note)"
for f in "${FILES[@]}"; do
  c=$(grep -cE '^:::(note|tip|info|caution|danger|warning)' "$f" 2>/dev/null || echo 0)
  echo "--- $f: $c"
  grep -nE '^:::(note|tip|info|caution|danger|warning)' "$f" 2>/dev/null | sed 's/^/    /'
done

hdr "Length (guide: under 50,000 characters)"
for f in "${FILES[@]}"; do
  c=$(wc -c <"$f" | tr -d ' ')
  [ "$c" -gt 50000 ] && echo "--- $f: $c OVER LIMIT" || echo "--- $f: $c"
done

hdr "Markdown links to verify with curl"
grep -ohE '\]\(/[^)]+\)' "${FILES[@]}" 2>/dev/null | sed 's/](\(.*\))/\1/' | sort -u

printf '\n\033[1mSweep finds mechanical issues only. Claims still need source verification.\033[0m\n'
