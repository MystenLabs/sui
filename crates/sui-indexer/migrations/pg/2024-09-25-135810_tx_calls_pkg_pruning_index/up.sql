CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_calls_pkg_tx_sequence_number
ON  tx_calls_pkg (tx_sequence_number);
