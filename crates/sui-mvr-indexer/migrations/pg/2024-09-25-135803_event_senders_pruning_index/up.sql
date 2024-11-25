CREATE INDEX CONCURRENTLY IF NOT EXISTS
    event_senders_tx_sequence_number
ON  event_senders (tx_sequence_number);
