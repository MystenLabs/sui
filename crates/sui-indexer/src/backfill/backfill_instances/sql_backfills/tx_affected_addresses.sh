# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

INDEXER=${INDEXER:-"sui-indexer"}
DB=${DB:-"postgres://postgres:postgrespw@localhost:5432/postgres"}
"$INDEXER" --database-url "$DB" run-back-fill "$1" "$2" sql "INSERT INTO tx_affected_addresses SELECT tx_sequence_number, sender AS affected, sender FROM tx_senders" tx_sequence_number
"$INDEXER" --database-url "$DB" run-back-fill "$1" "$2" sql "INSERT INTO tx_affected_addresses SELECT tx_sequence_number, recipient AS affected, sender FROM tx_recipients" tx_sequence_number
