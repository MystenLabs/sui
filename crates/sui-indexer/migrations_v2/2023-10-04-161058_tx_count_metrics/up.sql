CREATE TABLE tx_count_metrics
(
    checkpoint_sequence_number          BIGINT  PRIMARY KEY,
    epoch                               BIGINT  NOT NULL,
    timestamp_ms                        BIGINT  NOT NULL,
    -- totals of the current tx batch
    total_transaction_blocks            BIGINT  NOT NULL,
    total_successful_transaction_blocks BIGINT  NOT NULL,
    total_successful_transactions       BIGINT  NOT NULL,
    -- below are rolling totals from genesis used by get_total_transactions API
    network_total_transaction_blocks            BIGINT  NOT NULL,
    network_total_successful_transactions       BIGINT  NOT NULL,
    network_total_successful_transaction_blocks BIGINT  NOT NULL
);
