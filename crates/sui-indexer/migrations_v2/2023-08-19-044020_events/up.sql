CREATE TABLE events
(
    tx_sequence_number          BIGINT       NOT NULL,
    event_sequence_number       BIGINT       NOT NULL,
    transaction_digest          bytea        NOT NULL,
    checkpoint_sequence_number  bigint       NOT NULL,
    -- array of SuiAddress in bytes. All signers of the transaction.
    senders                     bytea[]      NOT NULL,
    -- bytes of the entry package ID
    package                     bytea        NOT NULL,
    -- entry module name
    module                      text         NOT NULL,
    -- StructTag in Display format
    event_type                  text         NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    -- bcs of the Event contents (Event.contents)
    bcs                         BYTEA        NOT NULL,
    PRIMARY KEY(tx_sequence_number, event_sequence_number)
);

CREATE INDEX events_package ON events (package, tx_sequence_number, event_sequence_number);
CREATE INDEX events_package_module ON events (package, module, tx_sequence_number, event_sequence_number);
CREATE INDEX events_event_type ON events (event_type text_pattern_ops, tx_sequence_number, event_sequence_number);
CREATE INDEX events_checkpoint_sequence_number ON events (checkpoint_sequence_number);
