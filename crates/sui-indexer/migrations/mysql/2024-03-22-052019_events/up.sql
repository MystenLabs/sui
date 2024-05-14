CREATE TABLE events
(
    tx_sequence_number          BIGINT       NOT NULL,
    event_sequence_number       BIGINT       NOT NULL,
    transaction_digest          BLOB         NOT NULL,
    checkpoint_sequence_number  BIGINT       NOT NULL,
    -- array of SuiAddress in bytes. All signers of the transaction.
    senders                     JSON         NOT NULL,
    -- bytes of the entry package ID. Notice that the package and module here
    -- are the package and module of the function that emitted the event, diffrent
    -- from the package and module of the event type.
    package                     BLOB         NOT NULL,
    -- entry module name
    module                      TEXT         NOT NULL,
    -- StructTag in Display format, fully qualified including type parameters
    event_type                  TEXT         NOT NULL,
    -- Components of the StructTag of the event type: package, module,
    -- name (name of the struct, without type parameters)
    event_type_package          BLOB         NOT NULL,
    event_type_module           TEXT         NOT NULL,
    event_type_name             TEXT         NOT NULL,
    -- timestamp of the checkpoint when the event was emitted
    timestamp_ms                BIGINT       NOT NULL,
    -- bcs of the Event contents (Event.contents)
    bcs                         MEDIUMBLOB   NOT NULL,
    PRIMARY KEY(tx_sequence_number, event_sequence_number, checkpoint_sequence_number)
) PARTITION BY RANGE (checkpoint_sequence_number) (
    PARTITION events_partition_0 VALUES LESS THAN MAXVALUE
);
CREATE INDEX events_package ON events (package(255), tx_sequence_number, event_sequence_number);
CREATE INDEX events_package_module ON events (package(255), module(255), tx_sequence_number, event_sequence_number);
CREATE INDEX events_event_type ON events (event_type(255), tx_sequence_number, event_sequence_number);
CREATE INDEX events_type_package_module_name ON events (event_type_package(128), event_type_module(128), event_type_name(128), tx_sequence_number, event_sequence_number);
CREATE INDEX events_checkpoint_sequence_number ON events (checkpoint_sequence_number);
