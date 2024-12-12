CREATE TABLE IF NOT EXISTS cp_mapping
(
    cp                                  BIGINT       PRIMARY KEY,
    -- The network total transactions at the end of this checkpoint subtracted by the number of
    -- transactions in the checkpoint.
    tx_lo                               BIGINT       NOT NULL,
    -- Exclusive upper transaction sequence number bound for this checkpoint, corresponds to the
    -- checkpoint's network total transactions. If this number is equal to `tx_lo`, then this
    -- checkpoint contains no transactions.
    tx_hi                               BIGINT       NOT NULL,
    -- The epoch this checkpoint belongs to.
    epoch                               BIGINT       NOT NULL
);
