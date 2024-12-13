CREATE TABLE IF NOT EXISTS cp_mapping
(
    cp_sequence_number                  BIGINT       PRIMARY KEY,
    -- The network total transactions at the end of this checkpoint subtracted by the number of
    -- transactions in the checkpoint.
    tx_lo                               BIGINT       NOT NULL,
    -- The epoch this checkpoint belongs to.
    epoch                               BIGINT       NOT NULL
);
