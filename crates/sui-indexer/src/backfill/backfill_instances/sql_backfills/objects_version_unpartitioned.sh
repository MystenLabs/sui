# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

INDEXER=${INDEXER:-"sui-indexer"}
DB=${DB:-"postgres://postgres:postgrespw@localhost:5432/postgres"}
"$INDEXER" --database-url "$DB" run-back-fill "$1" "$2" sql "INSERT INTO objects_version_unpartitioned (object_id, object_version, cp_sequence_number) SELECT object_id, object_version, checkpoint_sequence_number FROM objects_history" checkpoint_sequence_number
