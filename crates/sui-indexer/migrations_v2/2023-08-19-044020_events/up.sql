CREATE TABLE events
(
    tx_sequence_number          BIGINT       NOT NULL,
    event_sequence_number       BIGINT       NOT NULL,
    transaction_digest          BLOB        NOT NULL,
    checkpoint_sequence_number  bigint       NOT NULL,
    -- array of SuiAddress in bytes. All signers of the transaction.
    senders                     JSON      NOT NULL,
    -- TODO: verify the real limit of package, module and event_type
    -- bytes of the entry package ID
    package                     VARCHAR(255)        NOT NULL,
    -- entry module name
    module                      VARCHAR(127)         NOT NULL,
    -- StructTag in Display format
    event_type                  VARCHAR(255)         NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    -- bcs of the Event contents (Event.contents)
    bcs                         BLOB        NOT NULL,
    PRIMARY KEY(tx_sequence_number, event_sequence_number)
);

-- CREATE INDEX events_senders ON events USING GIN(senders);
CREATE INDEX events_package_module ON events (package, module);
CREATE INDEX events_event_type ON events (event_type);
CREATE INDEX events_checkpoint_sequence_number ON events (checkpoint_sequence_number);
