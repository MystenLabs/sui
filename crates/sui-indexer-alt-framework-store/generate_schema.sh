#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Update sui-indexer's generated src/schema.rs based on the schema after
# running all its migrations on a clean database. Expects the first argument to
# be a port to run the temporary database on (defaults to 5433).

set -x
set -e

if ! command -v git &> /dev/null; then
    echo "Please install git: e.g. brew install git" >&2
    exit 1
fi

for PG in psql initdb postgres pg_isready pg_ctl; do
    if ! command -v $PG &> /dev/null; then
        echo "Could not find $PG. Please install postgres: e.g. brew install postgresql@15" >&2
        exit 1
    fi
done

if ! command -v diesel &> /dev/null; then
    echo "Please install diesel: e.g. cargo install diesel_cli --features postgres" >&2
    exit 1
fi

REPO=$(git rev-parse --show-toplevel)

# Create a temporary directory to store the ephemeral DB.
TMP=$(mktemp -d)

# Set-up a trap to clean everything up on EXIT (stop DB, delete temp directory)
function cleanup {
  pg_ctl stop -D "$TMP" -mfast
  set +x
  echo "Postgres STDOUT:"
  cat "$TMP/db.stdout"
  echo "Postgres STDERR:"
  cat "$TMP/db.stderr"
  set -x
  rm -rf "$TMP"
}
trap cleanup EXIT

# Create a new database in the temporary directory
initdb -D "$TMP" --user postgres

# Run the DB in the background, on the port provided and capture its output
PORT=${1:-5433}
postgres -D "$TMP" -p "$PORT" -c unix_socket_directories=                      \
   > "$TMP/db.stdout"                                                          \
  2> "$TMP/db.stderr"                                                          &

# Wait for postgres to report as ready
RETRIES=0
while ! pg_isready -p "$PORT" --host "localhost" --username "postgres"; do
  if [ $RETRIES -gt 5 ]; then
    echo "Postgres failed to start" >&2
    exit 1
  fi
  sleep 1
  RETRIES=$((RETRIES + 1))
done

# Run all migrations on the new database, for the framework and the indexer
diesel migration run                                                          \
  --database-url "postgres://postgres:postgrespw@localhost:$PORT"             \
  --migration-dir "$REPO/crates/sui-indexer-alt-framework/migrations"

diesel migration run                                                          \
  --database-url "postgres://postgres:postgrespw@localhost:$PORT"             \
  --migration-dir "$REPO/crates/sui-indexer-alt-schema/migrations"

# Generate the schema.rs file, excluding framework tables and including the
# copyright notice.
diesel print-schema                                                           \
  --database-url "postgres://postgres:postgrespw@localhost:$PORT"             \
  --patch-file "$REPO/crates/sui-indexer-alt-schema/schema.patch"             \
  > "$REPO/crates/sui-indexer-alt-schema/src/schema.rs"
