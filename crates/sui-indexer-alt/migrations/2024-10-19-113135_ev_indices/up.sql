CREATE TABLE IF NOT EXISTS ev_emit_pkg
(
    package                     BYTEA,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_emit_pkg_tx_sequence_number
ON ev_emit_pkg (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_emit_pkg_sender
ON ev_emit_pkg (sender, package, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_emit_mod
(
    package                     BYTEA,
    module                      TEXT,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, module, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_emit_mod_tx_sequence_number
ON ev_emit_mod (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_emit_mod_sender
ON ev_emit_mod (sender, package, module, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_struct_pkg
(
    package                     BYTEA,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_struct_pkg_tx_sequence_number
ON ev_struct_pkg (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_pkg_sender
ON ev_struct_pkg (sender, package, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_struct_mod
(
    package                     BYTEA,
    module                      TEXT,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, module, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_struct_mod_tx_sequence_number
ON ev_struct_mod (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_mod_sender
ON ev_struct_mod (sender, package, module, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_struct_name
(
    package                     BYTEA,
    module                      TEXT,
    name                        TEXT,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, module, name, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_struct_name_tx_sequence_number
ON ev_struct_name (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_name_sender
ON ev_struct_name (sender, package, module, name, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_struct_inst
(
    package                     BYTEA,
    module                      TEXT,
    name                        TEXT,
    -- BCS encoded array of TypeTags for type parameters.
    instantiation               BYTEA,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, module, instantiation, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_struct_inst_tx_sequence_number
ON ev_struct_inst (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_inst_sender
ON ev_struct_inst (sender, package, module, instantiation, tx_sequence_number);
