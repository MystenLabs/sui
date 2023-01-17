CREATE TABLE checkpoints (
    sequence_number BIGINT PRIMARY KEY,
    content_digest VARCHAR(255) NOT NULL,
    epoch BIGINT NOT NULL,
    -- derived from gas cost summary
    total_gas_cost BIGINT NOT NULL,
    total_computation_cost BIGINT NOT NULL,
    total_storage_cost BIGINT NOT NULL,
    total_storage_rebate BIGINT NOT NULL,
    total_transactions BIGINT NOT NULL,
    previous_digest VARCHAR(255),
    next_epoch_committee TEXT,
    UNIQUE(sequence_number) 
);

CREATE TABLE checkpoint_logs (
    next_cursor_sequence_number BIGINT PRIMARY KEY
);

INSERT INTO checkpoint_logs (next_cursor_sequence_number) VALUES (0);
