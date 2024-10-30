CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_calls_mod_tx_sequence_number
ON  tx_calls_mod (tx_sequence_number);
