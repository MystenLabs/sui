CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_struct_package_tx_sequence_number
ON  event_struct_package (tx_sequence_number);
