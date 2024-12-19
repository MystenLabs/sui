-- This table maps a checkpoint sequence number to the containing epoch and first transaction
-- sequence number in the checkpoint.
CREATE TABLE IF NOT EXISTS cp_sequence_numbers
(
    cp_sequence_number                  BIGINT       PRIMARY KEY,
    -- The network total transactions at the end of this checkpoint subtracted by the number of
    -- transactions in the checkpoint.
    tx_lo                               BIGINT       NOT NULL,
    -- The epoch this checkpoint belongs to.
    epoch                               BIGINT       NOT NULL
);
