CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_changed_objects_tx_sequence_number
ON  tx_changed_objects (tx_sequence_number);
