-- TODO: modify queries in indexer reader to take advantage of the new indices
CREATE TABLE events
(
    tx_sequence_number          BIGINT       NOT NULL,
    event_sequence_number       BIGINT       NOT NULL,
    transaction_digest          bytea        NOT NULL,
    -- array of SuiAddress in bytes. All signers of the transaction.
    senders                     bytea[]      NOT NULL,
    -- bytes of the entry package ID. Notice that the package and module here
    -- are the package and module of the function that emitted the event, diffrent
    -- from the package and module of the event type.
    package                     bytea        NOT NULL,
    -- entry module name
    module                      text         NOT NULL,
    -- StructTag in Display format, fully qualified including type parameters
    event_type                  text         NOT NULL,
    -- timestamp of the checkpoint when the event was emitted
    timestamp_ms                BIGINT       NOT NULL,
    -- bcs of the Event contents (Event.contents)
    bcs                         BYTEA        NOT NULL,
    PRIMARY KEY(tx_sequence_number, event_sequence_number)
) PARTITION BY RANGE (tx_sequence_number);
CREATE TABLE events_partition_0 PARTITION OF events FOR VALUES FROM (0) TO (MAXVALUE);
CREATE INDEX events_package ON events (package, tx_sequence_number, event_sequence_number);
CREATE INDEX events_package_module ON events (package, module, tx_sequence_number, event_sequence_number);
CREATE INDEX events_event_type ON events (event_type text_pattern_ops, tx_sequence_number, event_sequence_number);
