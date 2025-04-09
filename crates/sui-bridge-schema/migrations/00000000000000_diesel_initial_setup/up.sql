CREATE TABLE token_transfer_data
(
    chain_id                    INT          NOT NULL,
    nonce                       BIGINT       NOT NULL,
    block_height                BIGINT       NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    txn_hash                    bytea        NOT NULL,
    sender_address              bytea        NOT NULL,
    destination_chain           INT          NOT NULL,
    recipient_address           bytea        NOT NULL,
    token_id                    INT          NOT NULL,
    amount                      BIGINT       NOT NULL,
    PRIMARY KEY(chain_id, nonce)
);
CREATE INDEX token_transfer_data_block_height ON token_transfer_data (block_height);
CREATE INDEX token_transfer_data_timestamp_ms ON token_transfer_data (timestamp_ms);
CREATE INDEX token_transfer_data_sender_address ON token_transfer_data (sender_address);
CREATE INDEX token_transfer_data_destination_chain ON token_transfer_data (destination_chain);
CREATE INDEX token_transfer_data_token_id ON token_transfer_data (token_id);

CREATE TABLE token_transfer
(
    chain_id                    INT          NOT NULL,
    nonce                       BIGINT       NOT NULL,
    status                      TEXT         NOT NULL,
    block_height                BIGINT       NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    txn_hash                    bytea        NOT NULL,
    txn_sender                  bytea        NOT NULL,
    gas_usage                   BIGINT       NOT NULL,
    data_source                 TEXT         NOT NULL,
    PRIMARY KEY(chain_id, nonce, status)
);
CREATE INDEX token_transfer_block_height ON token_transfer (block_height);
CREATE INDEX token_transfer_timestamp_ms ON token_transfer (timestamp_ms);

CREATE TABLE progress_store
(
    task_name                   TEXT          PRIMARY KEY,
    checkpoint                  BIGINT        NOT NULL
);

CREATE TABLE sui_progress_store
(
    id                           INT          PRIMARY KEY, -- dummy value
    txn_digest                   bytea        NOT NULL
);
