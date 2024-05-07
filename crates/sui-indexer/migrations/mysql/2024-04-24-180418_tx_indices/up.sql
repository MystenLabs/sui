-- Your SQL goes here
CREATE TABLE tx_senders (
                            cp_sequence_number          BIGINT       NOT NULL,
                            tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
                            sender                      BLOB        NOT NULL,
                            PRIMARY KEY(sender(255), tx_sequence_number, cp_sequence_number)
);
CREATE INDEX tx_senders_tx_sequence_number_index ON tx_senders (tx_sequence_number, cp_sequence_number);

-- Your SQL goes here
CREATE TABLE tx_recipients (
                               cp_sequence_number          BIGINT       NOT NULL,
                               tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
                               recipient                   BLOB        NOT NULL,
                               PRIMARY KEY(recipient(255), tx_sequence_number)
);
CREATE INDEX tx_recipients_tx_sequence_number_index ON tx_recipients (tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_input_objects (
                                  cp_sequence_number          BIGINT       NOT NULL,
                                  tx_sequence_number          BIGINT       NOT NULL,
    -- Object ID in bytes.
                                  object_id                   BLOB        NOT NULL,
                                  PRIMARY KEY(object_id(255), tx_sequence_number, cp_sequence_number)
);

CREATE TABLE tx_changed_objects (
                                    cp_sequence_number          BIGINT       NOT NULL,
                                    tx_sequence_number          BIGINT       NOT NULL,
    -- Object Id in bytes.
                                    object_id                   BLOB        NOT NULL,
                                    PRIMARY KEY(object_id(255), tx_sequence_number)
);

CREATE TABLE tx_calls (
                          cp_sequence_number          BIGINT       NOT NULL,
                          tx_sequence_number          BIGINT       NOT NULL,
                          package                     BLOB        NOT NULL,
                          module                      TEXT         NOT NULL,
                          func                        TEXT         NOT NULL,
    -- 1. Using Primary Key as a unique index.
    -- 2. Diesel does not like tables with no primary key.
                          PRIMARY KEY(package(255), tx_sequence_number, cp_sequence_number)
);

CREATE INDEX tx_calls_module ON tx_calls (package(255), module(255), tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_calls_func ON tx_calls (package(255), module(255), func(255), tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_calls_tx_sequence_number ON tx_calls (tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_digests (
                            tx_digest                   BLOB         NOT NULL,
                            cp_sequence_number          BIGINT       NOT NULL,
                            tx_sequence_number          BIGINT       NOT NULL,
                            PRIMARY KEY(tx_digest(255))
);