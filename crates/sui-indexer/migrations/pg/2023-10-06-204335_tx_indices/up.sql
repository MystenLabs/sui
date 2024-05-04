CREATE TABLE tx_senders (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    sender                      BYTEA        NOT NULL,
    -- SystemTransaction/ProgrammableTransaction. See types.rs
    transaction_kind            smallint     NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_senders_tx_sequence_number_index ON tx_senders (tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_senders_transaction_kind_index ON tx_senders (sender, transaction_kind, tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_recipients (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    recipient                   BYTEA        NOT NULL,
    -- SystemTransaction/ProgrammableTransaction. See types.rs
    transaction_kind            smallint     NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_recipients_tx_sequence_number_index ON tx_recipients (tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_recipients_transaction_kind_index ON tx_recipients (recipient, transaction_kind, tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_input_objects (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object ID in bytes.
    object_id                   BYTEA        NOT NULL,
    address                     BYTEA        NOT NULL,
    rel                         smallint     NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_input_objects_addr_index ON tx_input_objects (object_id, address, tx_sequence_number, rel);

CREATE TABLE tx_changed_objects (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object Id in bytes.
    object_id                   BYTEA        NOT NULL,
    address                     BYTEA        NOT NULL,
    rel                         smallint     NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_changed_objects_addr_index ON tx_changed_objects (object_id, address, tx_sequence_number, rel);

CREATE TABLE tx_calls (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    func                        TEXT         NOT NULL,
    address                     BYTEA        NOT NULL,
    rel                         smallint     NOT NULL,
    -- 1. Using Primary Key as a unique index.
    -- 2. Diesel does not like tables with no primary key.
    PRIMARY KEY(package, module, func, address, tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_calls_pkg_tx ON tx_calls (package, tx_sequence_number);
CREATE INDEX tx_calls_pkg_addr_tx_rel ON tx_calls (package, address, tx_sequence_number, rel);
CREATE INDEX tx_calls_pkg_module_tx ON tx_calls (package, module, tx_sequence_number);
CREATE INDEX tx_calls_pkg_module_addr_tx_rel ON tx_calls (package, module, address, tx_sequence_number, rel);
CREATE INDEX tx_calls_module ON tx_calls (package, module, tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_calls_tx_sequence_number ON tx_calls (tx_sequence_number, cp_sequence_number);

-- un-partitioned table for tx_digest -> (cp_sequence_number, tx_sequence_number) lookup.
CREATE TABLE tx_digests (
    tx_digest                   BYTEA        PRIMARY KEY,
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL
);
