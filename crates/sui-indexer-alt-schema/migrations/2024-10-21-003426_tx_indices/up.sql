CREATE TABLE IF NOT EXISTS tx_affected_addresses
(
    affected                    BYTEA        NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY (affected, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_affected_addresses_tx_sequence_number
ON tx_affected_addresses (tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_affected_addresses_sender
ON tx_affected_addresses (sender, affected, tx_sequence_number);

CREATE TABLE IF NOT EXISTS tx_digests
(
    tx_sequence_number          BIGINT       PRIMARY KEY,
    tx_digest                   BYTEA        NOT NULL
);

CREATE TABLE IF NOT EXISTS tx_kinds
(
    tx_kind                     SMALLINT     NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    PRIMARY KEY (tx_kind, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_kinds_tx_sequence_number
ON tx_kinds (tx_sequence_number);

CREATE TABLE IF NOT EXISTS tx_calls
(
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    function                    TEXT         NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY (package, module, function, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_calls_tx_sequence_number
ON tx_calls (tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_fun_sender
ON tx_calls (sender, package, module, function, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_mod
ON tx_calls (package, module, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_mod_sender
ON tx_calls (sender, package, module, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_pkg
ON tx_calls (package, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_pkg_sender
ON tx_calls (sender, package, tx_sequence_number);
