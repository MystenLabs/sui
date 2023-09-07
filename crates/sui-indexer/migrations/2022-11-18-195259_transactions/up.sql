CREATE TABLE transactions (
    id                          BIGINT AUTO_INCREMENT PRIMARY KEY,
    transaction_digest          VARCHAR(44)  NOT NULL,
    sender                      VARCHAR(255) NOT NULL,
    recipients                  JSON       NOT NULL,
    checkpoint_sequence_number  BIGINT,
    timestamp_ms                BIGINT,
    transaction_kind            TEXT         NOT NULL,
    transaction_count           BIGINT       NOT NULL,
    execution_success           TINYINT(1)      NOT NULL,
    -- object related
    created                     JSON       NOT NULL,
    mutated                     JSON       NOT NULL,
    deleted                     JSON       NOT NULL,
    unwrapped                   JSON       NOT NULL,
    wrapped                     JSON       NOT NULL,
    -- each move call is <package>::<module>::<function>
    move_calls                  JSON       NOT NULL,
    -- gas object related
    gas_object_id               VARCHAR(66)  NOT NULL,
    gas_object_sequence         BIGINT       NOT NULL,
    gas_object_digest           VARCHAR(66)  NOT NULL,
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
    raw_transaction             BLOB        NOT NULL,
    transaction_content         TEXT         NOT NULL,
    transaction_effects_content TEXT         NOT NULL,
    confirmed_local_execution   TINYINT(1),
    UNIQUE (transaction_digest)
);

CREATE INDEX transactions_transaction_digest ON transactions (transaction_digest);
CREATE INDEX transactions_timestamp_ms ON transactions (timestamp_ms);
CREATE INDEX transactions_sender ON transactions (sender);
CREATE INDEX transactions_checkpoint_sequence_number ON transactions (checkpoint_sequence_number);
CREATE INDEX transactions_execution_success ON transactions (execution_success);
