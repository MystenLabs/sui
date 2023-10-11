-- Your SQL goes here
CREATE TABLE tx_recipients (
    tx_sequence_number          BIGINT       NOT NULL,
<<<<<<< HEAD
=======
    checkpoint_sequence_number  BIGINT       NOT NULL,
>>>>>>> 86f2b5c57 (rework some indices)
    -- SuiAddress in bytes.
    recipient                   BYTEA        NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number)
);
