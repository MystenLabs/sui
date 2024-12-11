CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_struct_module_tx_sequence_number
ON  event_struct_module (tx_sequence_number);
