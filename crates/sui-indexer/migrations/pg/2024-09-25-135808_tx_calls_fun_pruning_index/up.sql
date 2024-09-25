CREATE INDEX CONCURRENTLY IF NOT EXISTS
    tx_calls_fun_tx_sequence_number
ON  tx_calls_fun (tx_sequence_number);
