CREATE TABLE IF NOT EXISTS tx_affected_objects
(
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object ID of the object touched by this transaction.
    affected                    BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(affected, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_affected_objects_tx_sequence_number
ON tx_affected_objects (tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_affected_objects_sender
ON tx_affected_objects (sender, affected, tx_sequence_number);
