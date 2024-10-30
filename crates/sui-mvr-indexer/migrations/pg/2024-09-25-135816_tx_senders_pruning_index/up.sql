CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_senders_tx_sequence_number
ON  tx_senders (tx_sequence_number);
