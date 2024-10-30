CREATE TABLE tx_senders (
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_senders_tx_sequence_number
    ON tx_senders (tx_sequence_number);

CREATE TABLE tx_recipients (
    tx_sequence_number          BIGINT       NOT NULL,
    recipient                   BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_recipients_sender
    ON tx_recipients (sender, recipient, tx_sequence_number);
