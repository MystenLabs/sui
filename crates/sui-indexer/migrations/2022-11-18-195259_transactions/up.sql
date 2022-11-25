CREATE TABLE transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_digest VARCHAR(64) NOT NULL,
    sender VARCHAR(64) NOT NULL,
    transaction_time TIMESTAMP,
    transaction_kinds TEXT[] NOT NULL,
    -- object related
    created TEXT[] NOT NULL,
    mutated TEXT[] NOT NULL,
    deleted TEXT[] NOT NULL,
    unwrapped TEXT[] NOT NULL,
    wrapped TEXT[] NOT NULL,
    -- gas object related
    gas_object_id VARCHAR(64) NOT NULL,
    gas_object_sequence BIGINT NOT NULL,
    gas_object_digest VARCHAR(64) NOT NULL,
    -- gas budget & cost related
    gas_budget BIGINT NOT NULL,
    total_gas_cost BIGINT NOT NULL,
    computation_cost BIGINT NOT NULL,
    storage_cost BIGINT NOT NULL,
    storage_rebate BIGINT NOT NULL
);
