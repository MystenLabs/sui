-- Your SQL goes here
CREATE TABLE tx_senders (
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number)
);
CREATE INDEX tx_senders_tx_sequence_number_index ON tx_senders (tx_sequence_number ASC);
