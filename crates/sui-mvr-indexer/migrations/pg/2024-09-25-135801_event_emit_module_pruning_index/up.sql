CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_emit_module_tx_sequence_number
ON  event_emit_module (tx_sequence_number);
