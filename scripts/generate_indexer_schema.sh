#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Update sui-indexer's generated src/schema.rs based on the schema after
# running all its migrations on a clean database. Expects the first argument to
# be a Postgres Database URL, defaulting to
#
#   postgres://postgres:postgrespw@localhost:5432
#
# if none is provided.

set -x
set -e

if ! command -v git &> /dev/null; then
    echo "Please install git: e.g. brew install git" >&2
    exit 1
fi

if ! command -v psql &> /dev/null; then
    echo "Please install psql: e.g. brew install postgresql@15" >&2
    exit 1
fi

if ! command -v diesel &> /dev/null; then
    echo "Please install diesel: e.g. cargo install diesel_cli --features postgres" >&2
    exit 1
fi

REPO=$(git rev-parse --show-toplevel)
DATABASE_URL=${1:-"postgres://postgres:postgrespw@localhost:5432"}

# Generate a unique DB name so that we don't risk stomping an existing DB. The
# name will include the PID of the script process and the current timestamp in
# seconds (calculated by `date` in a cross-platform compatible way):
DB_NAME="sui_indexer_$$_$(date +'%s')"

# Create a new database, and drop it on EXIT
psql -c "CREATE DATABASE $DB_NAME" "$DATABASE_URL"
trap "psql -c 'DROP DATABASE $DB_NAME' '$DATABASE_URL'" EXIT

# Run all migrations on the new database
diesel migration run                                                          \
  --database-url "postgres://postgres:postgrespw@localhost:5432/$DB_NAME"     \
  --migration-dir "$REPO/crates/sui-indexer/migrations/pg"

# Generate the schema.rs file, excluding partition tables and including the
# copyright notice.
diesel print-schema                                                           \
  --database-url "postgres://postgres:postgrespw@localhost:5432/$DB_NAME"     \
  --patch-file "$REPO/crates/sui-indexer/src/schema.patch"                    \
  --except-tables "^objects_version_|_partition_"                             \
  > "$REPO/crates/sui-indexer/src/schema.rs"
