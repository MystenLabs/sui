-- Your SQL goes here
CREATE TABLE tx_senders (
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number)
);
