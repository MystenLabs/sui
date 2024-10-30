CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_digests_tx_sequence_number
ON  tx_digests (tx_sequence_number);
