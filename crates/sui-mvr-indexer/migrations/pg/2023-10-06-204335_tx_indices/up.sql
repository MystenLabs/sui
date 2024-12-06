CREATE TABLE tx_senders (
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number)
);

CREATE TABLE tx_recipients (
    tx_sequence_number          BIGINT       NOT NULL,
    recipient                   BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number)
);
CREATE INDEX tx_recipients_sender ON tx_recipients (sender, recipient, tx_sequence_number);

CREATE TABLE tx_input_objects (
    tx_sequence_number          BIGINT       NOT NULL,
    object_id                   BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number)
);
CREATE INDEX tx_input_objects_sender ON tx_input_objects (sender, object_id, tx_sequence_number);

CREATE TABLE tx_changed_objects (
    tx_sequence_number          BIGINT       NOT NULL,
    object_id                   BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number)
);
CREATE INDEX tx_changed_objects_sender ON tx_changed_objects (sender, object_id, tx_sequence_number);

CREATE TABLE tx_calls_pkg (
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(package, tx_sequence_number)
);
CREATE INDEX tx_calls_pkg_sender ON tx_calls_pkg (sender, package, tx_sequence_number);

CREATE TABLE tx_calls_mod (
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(package, module, tx_sequence_number)
);
CREATE INDEX tx_calls_mod_sender ON tx_calls_mod (sender, package, module, tx_sequence_number);

CREATE TABLE tx_calls_fun (
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    func                        TEXT         NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(package, module, func, tx_sequence_number)
);
CREATE INDEX tx_calls_fun_sender ON tx_calls_fun (sender, package, module, func, tx_sequence_number);

CREATE TABLE tx_digests (
    tx_digest                   BYTEA        PRIMARY KEY,
    tx_sequence_number          BIGINT       NOT NULL
);

CREATE TABLE tx_kinds (
    tx_sequence_number          BIGINT       NOT NULL,
    tx_kind                     SMALLINT     NOT NULL,
    PRIMARY KEY(tx_kind, tx_sequence_number)
);
