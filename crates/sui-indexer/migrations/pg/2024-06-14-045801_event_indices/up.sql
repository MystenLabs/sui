CREATE TABLE event_emit_package
(
    package                     BYTEA   NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_emit_package_sender ON event_emit_package (sender, package, tx_sequence_number, event_sequence_number);

CREATE TABLE event_emit_module
(
    package                     BYTEA   NOT NULL,
    module                      TEXT    NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, module, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_emit_module_sender ON event_emit_module (sender, package, module, tx_sequence_number, event_sequence_number);

CREATE TABLE event_struct_package
(
    package                     BYTEA   NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_struct_package_sender ON event_struct_package (sender, package, tx_sequence_number, event_sequence_number);


CREATE TABLE event_struct_module
(
    package                     BYTEA   NOT NULL,
    module                      TEXT    NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, module, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_struct_module_sender ON event_struct_module (sender, package, module, tx_sequence_number, event_sequence_number);

CREATE TABLE event_struct_name
(
    package                     BYTEA   NOT NULL,
    module                      TEXT    NOT NULL,
    type_name                   TEXT    NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, module, type_name, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_struct_name_sender ON event_struct_name (sender, package, module, type_name, tx_sequence_number, event_sequence_number);

CREATE TABLE event_struct_instantiation
(
    package                     BYTEA   NOT NULL,
    module                      TEXT    NOT NULL,
    type_instantiation          TEXT    NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    sender                      BYTEA   NOT NULL,
    PRIMARY KEY(package, module, type_instantiation, tx_sequence_number, event_sequence_number)
);
CREATE INDEX event_struct_instantiation_sender ON event_struct_instantiation (sender, package, module, type_instantiation, tx_sequence_number, event_sequence_number);

CREATE TABLE event_senders
(
    sender                      BYTEA   NOT NULL,
    tx_sequence_number          BIGINT  NOT NULL,
    event_sequence_number       BIGINT  NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number, event_sequence_number)
);
