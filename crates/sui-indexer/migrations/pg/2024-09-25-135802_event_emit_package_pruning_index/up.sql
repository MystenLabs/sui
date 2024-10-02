CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_emit_package_tx_sequence_number
ON  event_emit_package (tx_sequence_number);
