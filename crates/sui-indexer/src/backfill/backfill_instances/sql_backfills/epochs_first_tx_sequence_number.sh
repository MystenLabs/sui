# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

INDEXER=${INDEXER:-"sui-indexer"}
DB=${DB:-"postgres://postgres:postgrespw@localhost:5432/postgres"}
"$INDEXER" --database-url "$DB" run-back-fill "$1" "$2" sql "WITH running_sum AS (
    SELECT
        epoch,
        COALESCE(SUM(epoch_total_transactions) OVER (ORDER BY epoch ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0) as calculated_first_tx
    FROM epochs
)
UPDATE epochs e
SET first_tx_sequence_number = r.calculated_first_tx
FROM running_sum r
WHERE e.epoch = r.epoch" e.epoch
