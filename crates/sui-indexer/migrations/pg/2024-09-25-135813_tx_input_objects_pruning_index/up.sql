CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_input_objects_tx_sequence_number
ON  tx_input_objects (tx_sequence_number);
