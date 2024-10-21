CREATE TABLE IF NOT EXISTS tx_affected_addresses
(
    affected                    BYTEA        NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(affected, tx_sequence_number)
);

CREATE INDEX IF NOT EXISTS tx_affected_addresses_tx_sequence_number ON tx_affected_addresses (tx_sequence_number);
CREATE INDEX IF NOT EXISTS tx_affected_addresses_sender ON tx_affected_addresses (sender, affected, tx_sequence_number);

CREATE TABLE IF NOT EXISTS tx_digests
(
    tx_digest                   BYTEA        PRIMARY KEY,
    tx_sequence_number          BIGINT       NOT NULL
);
CREATE INDEX IF NOT EXISTS tx_digests_tx_sequence_number ON tx_digests (tx_sequence_number);

CREATE TABLE IF NOT EXISTS tx_kinds
(
    tx_kind                     SMALLINT     NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    PRIMARY KEY(tx_kind, tx_sequence_number)
);
CREATE INDEX IF NOT EXISTS tx_kinds_tx_sequence_number ON tx_kinds (tx_sequence_number);

CREATE TABLE IF NOT EXISTS tx_calls_fun
(
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    func                        TEXT         NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(package, module, func, tx_sequence_number)
);
CREATE INDEX IF NOT EXISTS tx_calls_fun_tx_sequence_number ON tx_calls_fun (tx_sequence_number);
CREATE INDEX IF NOT EXISTS tx_calls_fun_sender ON tx_calls_fun (sender, package, module, func, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_mod ON tx_calls_fun (package, module, tx_sequence_number);
CREATE INDEX IF NOT EXISTS tx_calls_mod_sender ON tx_calls_fun (sender, package, module, tx_sequence_number);

CREATE INDEX IF NOT EXISTS tx_calls_pkg ON tx_calls_fun (package, tx_sequence_number);
CREATE INDEX IF NOT EXISTS tx_calls_pkg_sender ON tx_calls_fun (sender, package, tx_sequence_number);
