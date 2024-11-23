# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

INDEXER=${INDEXER:-"sui-indexer"}
DB=${DB:-"postgres://postgres:postgrespw@localhost:5432/postgres"}
"$INDEXER" --database-url "$DB" run-back-fill "$1" "$2" sql "INSERT INTO full_objects_history (object_id, object_version, serialized_object) SELECT object_id, object_version, serialized_object FROM objects_history" checkpoint_sequence_number
