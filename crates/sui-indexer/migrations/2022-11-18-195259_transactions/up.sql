DO $$
BEGIN
-- SuiAddress and ObjectId type, 0x + 64 chars hex string
CREATE DOMAIN address VARCHAR(66);
-- Max char length for base58 encoded digest
CREATE DOMAIN base58digest VARCHAR(44);
EXCEPTION
    WHEN duplicate_object THEN
        -- Domain already exists, do nothing
        NULL;
END $$;


CREATE TABLE transactions (
    id                          BIGSERIAL PRIMARY KEY,
    transaction_digest          base58digest NOT NULL,
    sender                      VARCHAR(255) NOT NULL,
    checkpoint_sequence_number  BIGINT,
    timestamp_ms                BIGINT,
    transaction_kind            TEXT         NOT NULL,
    transaction_count           BIGINT       NOT NULL,
    execution_success           BOOLEAN      NOT NULL,
    -- gas object related
    gas_object_id               address      NOT NULL,
    gas_object_sequence         BIGINT       NOT NULL,
    gas_object_digest           address      NOT NULL,
    -- gas budget & cost related
    gas_budget                  BIGINT       NOT NULL,
    total_gas_cost              BIGINT       NOT NULL,
    computation_cost            BIGINT       NOT NULL,
    storage_cost                BIGINT       NOT NULL,
    storage_rebate              BIGINT       NOT NULL,
    non_refundable_storage_fee  BIGINT       NOT NULL,
    -- gas price from transaction data,
    -- not the reference gas price
    gas_price                   BIGINT       NOT NULL,
    -- BCS serialized SenderSignedData
    raw_transaction             bytea        NOT NULL,
    transaction_effects_content TEXT         NOT NULL,
    confirmed_local_execution   BOOLEAN,
    UNIQUE (transaction_digest)
);

CREATE INDEX transactions_transaction_digest ON transactions (transaction_digest);
CREATE INDEX transactions_timestamp_ms ON transactions (timestamp_ms);
CREATE INDEX transactions_sender ON transactions (sender);
CREATE INDEX transactions_checkpoint_sequence_number ON transactions (checkpoint_sequence_number);
CREATE INDEX transactions_execution_success ON transactions (execution_success);
CREATE INDEX transactions_transaction_kind ON transactions (transaction_kind);
