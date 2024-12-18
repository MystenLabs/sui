CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_struct_name_tx_sequence_number
ON  event_struct_name (tx_sequence_number);
