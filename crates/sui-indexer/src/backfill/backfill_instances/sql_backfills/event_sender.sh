# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

sui-indexer --database-url postgres://postgres@localhost:5432/postgres run-back-fill "$1" "$2" sql "UPDATE events SET sender = CASE WHEN cardinality(senders) > 0 THEN senders[1] ELSE NULL END" checkpoint_sequence_number
