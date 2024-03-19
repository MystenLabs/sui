CREATE TABLE tx_count_metrics
(
    checkpoint_sequence_number          BIGINT  PRIMARY KEY,
    epoch                               BIGINT  NOT NULL,
    timestamp_ms                        BIGINT  NOT NULL,
    total_transaction_blocks            BIGINT  NOT NULL,
    total_successful_transaction_blocks BIGINT  NOT NULL,
    total_successful_transactions       BIGINT  NOT NULL
);
-- epoch for peak 30D TPS filter
CREATE INDEX tx_count_metrics_epoch ON tx_count_metrics (epoch);
-- timestamp for timestamp grouping, in case multiple checkpoints have the same timestamp
CREATE INDEX tx_count_metrics_timestamp_ms ON tx_count_metrics (timestamp_ms);
