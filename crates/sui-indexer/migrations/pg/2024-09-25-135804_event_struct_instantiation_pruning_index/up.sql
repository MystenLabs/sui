CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_struct_instantiation_tx_sequence_number
ON  event_struct_instantiation (tx_sequence_number);
