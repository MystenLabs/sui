CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_recipients_tx_sequence_number
ON  tx_recipients (tx_sequence_number);
