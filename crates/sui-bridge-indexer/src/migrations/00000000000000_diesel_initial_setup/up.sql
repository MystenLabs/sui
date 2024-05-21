CREATE TABLE token_transfer_data
(
    chain_id                    INT          NOT NULL,
    nonce                       BIGINT       NOT NULL,
    sender_address              bytea        NOT NULL,
    destination_chain           INT          NOT NULL,
    recipient_address           bytea        NOT NULL,
    token_id                    INT          NOT NULL,
    amount                      BIGINT       NOT NULL,
    PRIMARY KEY(chain_id, nonce)
);
CREATE INDEX token_transfer_data_destination_chain ON token_transfer_data (destination_chain);
CREATE INDEX token_transfer_data_token_id ON token_transfer_data (token_id);

CREATE TABLE token_transfer
(
    chain_id                    INT          NOT NULL,
    nonce                       BIGINT       NOT NULL,
    block_height                BIGINT       NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    txn_hash                    bytea        NOT NULL,
    status                      TEXT         NOT NULL,
    gas_usage                   BIGINT       NOT NULL,
    PRIMARY KEY(chain_id, nonce)
);