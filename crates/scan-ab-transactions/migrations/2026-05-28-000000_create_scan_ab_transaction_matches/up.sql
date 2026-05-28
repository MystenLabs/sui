-- Copyright (c) Mysten Labs, Inc.
-- SPDX-License-Identifier: Apache-2.0

CREATE TABLE scan_ab_transaction_matches (
    tx_sequence_number BIGINT PRIMARY KEY,
    checkpoint_sequence_number BIGINT NOT NULL,
    tx_digest TEXT NOT NULL
);

CREATE INDEX scan_ab_transaction_matches_checkpoint_sequence_number
    ON scan_ab_transaction_matches (checkpoint_sequence_number);
