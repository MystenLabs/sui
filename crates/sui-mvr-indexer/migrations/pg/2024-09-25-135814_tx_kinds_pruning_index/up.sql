CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_kinds_tx_sequence_number
ON  tx_kinds (tx_sequence_number);
