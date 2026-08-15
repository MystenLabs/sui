#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

spec_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ledger="$spec_dir/docs/ASSUMPTIONS.md"
defined_ids_file="$(mktemp)"
referenced_ids_file="$(mktemp)"
refinement_ids_file="$(mktemp)"

cleanup() {
    rm -f -- "$defined_ids_file" "$referenced_ids_file" "$refinement_ids_file"
}
trap cleanup EXIT

sed -nE 's/^## (ASM-[A-Z0-9-]+)$/\1/p' "$ledger" | sort >"$defined_ids_file"
defined_count="$(wc -l <"$defined_ids_file" | tr -d '[:space:]')"

if [ "$defined_count" -eq 0 ]; then
    echo "The assumption ledger does not define an ASM identifier." >&2
    exit 1
fi

duplicate_ids="$(uniq -d "$defined_ids_file")"
if [ -n "$duplicate_ids" ]; then
    echo "The assumption ledger defines an identifier more than once:" >&2
    echo "$duplicate_ids" >&2
    exit 1
fi

required_fields=(
    "Claim"
    "Type"
    "Status"
    "Effect if false"
    "Lean use"
    "Rust evidence"
    "Discharge"
)
for field in "${required_fields[@]}"; do
    field_count="$(grep -Fc -- "- **$field:**" "$ledger")"
    if [ "$field_count" -ne "$defined_count" ]; then
        echo "Expected one '$field' field for each assumption; found $field_count." >&2
        exit 1
    fi
done

status_values=(
    "Discharged in Lean"
    "Enforced in Rust"
    "Partially verified"
    "Environmental assumption"
    "Open proof obligation"
    "Abstraction gap"
    "Accepted modeling assumption"
    "Known mismatch"
)
status_total=0
for status_value in "${status_values[@]}"; do
    expected_count="$(
        awk -F '|' -v wanted="$status_value" '
            {
                name = $2
                count = $3
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", name)
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", count)
                if (name == wanted) print count
            }
        ' "$ledger"
    )"
    actual_count="$(grep -Fc -- "- **Status:** $status_value." "$ledger" || true)"
    if [ -z "$expected_count" ] || [ "$actual_count" -ne "$expected_count" ]; then
        echo "The summary count for status '$status_value' is not correct." >&2
        exit 1
    fi
    status_total=$((status_total + actual_count))
done

if [ "$status_total" -ne "$defined_count" ]; then
    echo "An assumption has an unknown or missing status." >&2
    exit 1
fi

rg --no-filename --only-matching 'ASM-[A-Z0-9-]+' \
    "$spec_dir/README.md" \
    "$spec_dir/design" \
    "$spec_dir/docs/PROOF_SCOPE.md" \
    "$spec_dir/docs/IMPLEMENTATION_GAPS.md" \
    "$spec_dir/lean/Mysticeti" |
    sort -u >"$referenced_ids_file"

unknown_ids="$(
    comm -13 "$defined_ids_file" "$referenced_ids_file"
)"
if [ -n "$unknown_ids" ]; then
    echo "These assumption identifiers are not in the ledger:" >&2
    echo "$unknown_ids" >&2
    exit 1
fi

unreferenced_ids="$(
    comm -23 "$defined_ids_file" "$referenced_ids_file"
)"
if [ -n "$unreferenced_ids" ]; then
    echo "These ledger identifiers have no Lean or design document reference:" >&2
    echo "$unreferenced_ids" >&2
    exit 1
fi

sed -nE 's/^\| `(REF-[A-Z0-9-]+)` \|.*/\1/p' "$ledger" |
    sort >"$refinement_ids_file"
refinement_count="$(wc -l <"$refinement_ids_file" | tr -d '[:space:]')"
if [ "$refinement_count" -eq 0 ]; then
    echo "The Lean-to-Rust refinement review is empty." >&2
    exit 1
fi

duplicate_refinement_ids="$(uniq -d "$refinement_ids_file")"
if [ -n "$duplicate_refinement_ids" ]; then
    echo "The Lean-to-Rust refinement review defines an identifier more than once:" >&2
    echo "$duplicate_refinement_ids" >&2
    exit 1
fi

echo "Assumption ledger check passed for $defined_count assumptions and $refinement_count Rust mappings."
