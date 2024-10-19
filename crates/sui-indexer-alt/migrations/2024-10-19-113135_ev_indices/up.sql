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

CREATE INDEX IF NOT EXISTS ev_emit_pkg
ON ev_emit_mod (package, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_emit_pkg_sender
ON ev_emit_mod (sender, package, tx_sequence_number);

CREATE TABLE IF NOT EXISTS ev_struct_inst
(
    package                     BYTEA,
    module                      TEXT,
    name                        TEXT,
    -- BCS encoded array of TypeTags for type parameters.
    instantiation               BYTEA,
    tx_sequence_number          BIGINT,
    sender                      BYTEA         NOT NULL,
    PRIMARY KEY(package, module, name, instantiation, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS ev_struct_inst_tx_sequence_number
ON ev_struct_inst (tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_inst_sender
ON ev_struct_inst (sender, package, module, name, instantiation, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_name
ON ev_struct_inst (package, module, name, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_name_sender
ON ev_struct_inst (sender, package, module, name, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_mod
ON ev_struct_inst (package, module, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_mod_sender
ON ev_struct_inst (sender, package, module, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_pkg
ON ev_struct_inst (package, tx_sequence_number);

CREATE INDEX IF NOT EXISTS ev_struct_pkg_sender
ON ev_struct_inst (sender, package, tx_sequence_number);
